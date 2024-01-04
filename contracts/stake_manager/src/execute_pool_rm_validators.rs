use std::ops::{Div, Mul, Sub};
use std::{collections::HashSet, vec};

use cosmwasm_std::{Delegation, DepsMut, Env, MessageInfo, Response, StdError, Uint128};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    interchain_queries::{
        check_query_type, get_registered_query, query_kv_result, types::QueryType,
        v045::types::Delegations,
    },
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronError, NeutronResult,
};

use crate::helper::gen_redelegate_txs;
use crate::state::{ValidatorUpdateStatus, INFO_OF_ICA_ID, POOLS};
use crate::{
    contract::{msg_with_sudo_callback, SudoPayload, TxType, DEFAULT_TIMEOUT_SECONDS},
    state::ADDR_DELEGATIONS_REPLY_ID,
};
use crate::{error_conversion::ContractError, state::REPLY_ID_TO_QUERY_ID};
use crate::{helper::min_ntrn_ibc_fee, state::POOL_VALIDATOR_STATUS};

pub fn execute_rm_pool_validators(
    mut deps: DepsMut<NeutronQuery>,
    _: Env,
    info: MessageInfo,
    pool_addr: String,
    validator_addrs: Vec<String>,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.as_ref().storage, pool_addr.clone())?;

    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_rm_pool_validators pool_info: {:?}",
            pool_info
        )
        .as_str(),
    );

    let mut pool_validator_status = POOL_VALIDATOR_STATUS.load(deps.storage, pool_addr.clone())?;
    if pool_validator_status.status == ValidatorUpdateStatus::Pending {
        deps.as_ref().api.debug(
            format!(
                "WASMDEBUG: execute_rm_pool_validators skip pool: {:?}",
                pool_addr
            )
            .as_str(),
        );
        return Err(NeutronError::Std(StdError::generic_err("status not allow")));
    }

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_rm_pool_validators pool_validator_status: {:?}",
            pool_validator_status
        )
        .as_str(),
    );

    // redelegate
    let contract_query_id = ADDR_DELEGATIONS_REPLY_ID.load(deps.storage, pool_addr.clone())?;
    let registered_query_id = REPLY_ID_TO_QUERY_ID.load(deps.storage, contract_query_id)?;

    // get info about the query
    let registered_query = get_registered_query(deps.as_ref(), registered_query_id)?;

    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Delegations structure from raw KV-storage values
    let delegations: Delegations = query_kv_result(deps.as_ref(), registered_query_id)?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_rm_pool_validators delegations: {:?}",
            delegations
        )
        .as_str(),
    );

    let filtered_delegations: Vec<Delegation> = delegations
        .delegations
        .into_iter()
        .filter(|delegation| validator_addrs.contains(&delegation.validator))
        .collect();

    let rm_validators_set: HashSet<_> = validator_addrs.clone().into_iter().collect();
    let now_validators_set: HashSet<_> = pool_info.validator_addrs.clone().into_iter().collect();

    // Find the difference
    let difference: HashSet<_> = now_validators_set.difference(&rm_validators_set).collect();
    let new_validators: Vec<String> = difference.into_iter().cloned().collect();

    let mut msgs = vec![];

    let validator_count = new_validators.len() as u128;

    if validator_count == 0 {
        return Err(NeutronError::Std(StdError::generic_err(
            "validator_count is zero",
        )));
    }

    for delegation in filtered_delegations {
        let stake_amount = delegation.amount.amount;
        deps.as_ref().api.debug(
            format!(
                "WASMDEBUG: execute_era_bond stake_amount: {}, validator_count is {}",
                stake_amount, validator_count
            )
            .as_str(),
        );

        if stake_amount.is_zero() {
            continue;
        }

        let amount_per_validator = stake_amount.div(Uint128::from(validator_count));
        let remainder = stake_amount.sub(amount_per_validator.mul(Uint128::new(validator_count)));

        deps.as_ref().api.debug(
            format!(
                "WASMDEBUG: execute_era_bond amount_per_validator: {}, remainder is {}",
                amount_per_validator, remainder
            )
            .as_str(),
        );

        for (index, target_validator) in new_validators.clone().into_iter().enumerate() {
            let mut amount_for_this_validator = amount_per_validator;

            // Add the remainder to the first validator
            if index == 0 {
                amount_for_this_validator += remainder;
            }

            deps.as_ref().api.debug(
                format!(
                    "Validator: {}, Bond: {}",
                    target_validator, amount_for_this_validator
                )
                .as_str(),
            );

            let any_msg = gen_redelegate_txs(
                pool_addr.clone(),
                delegation.validator.clone(),
                target_validator.clone(),
                pool_info.remote_denom.clone(),
                amount_for_this_validator,
            );

            msgs.push(any_msg);
        }
    }
    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

    // let remove_msg_old_query = NeutronMsg::remove_interchain_query(registere_query_id);
    let mut resp = Response::default(); // .add_message(remove_msg_old_query)

    if !msgs.is_empty() {
        let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);

        let cosmos_msg = NeutronMsg::submit_tx(
            pool_ica_info.ctrl_connection_id.clone(),
            pool_info.ica_id.clone(),
            msgs,
            "".to_string(),
            DEFAULT_TIMEOUT_SECONDS,
            fee,
        );

        let new_validator_list_str = new_validators
            .clone()
            .iter()
            .map(|index| index.to_string())
            .collect::<Vec<String>>()
            .join("_");

        let submsg_redelegate = msg_with_sudo_callback(
            deps.branch(),
            cosmos_msg,
            SudoPayload {
                port_id: pool_ica_info.ctrl_port_id,
                pool_addr: pool_ica_info.ica_addr.clone(),
                message: new_validator_list_str,
                tx_type: TxType::UpdateValidators,
            },
        )?;

        pool_validator_status.status = ValidatorUpdateStatus::Pending;

        pool_validator_status.status = ValidatorUpdateStatus::WaitQueryUpdate;
        resp = resp.add_submessage(submsg_redelegate)
    } else {
        pool_info.validator_addrs = new_validators;
        pool_validator_status.status = ValidatorUpdateStatus::Success;
    }

    POOL_VALIDATOR_STATUS.save(deps.storage, pool_addr.clone(), &pool_validator_status)?;
    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;

    Ok(resp)
}

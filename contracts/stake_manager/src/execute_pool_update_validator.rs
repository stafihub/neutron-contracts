use std::vec;

use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdError, StdResult, SubMsg};

use neutron_sdk::interchain_queries::v045::new_register_delegator_delegations_query_msg;
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
use crate::state::{get_next_icq_reply_id, QueryKind, ValidatorUpdateStatus, POOLS};
use crate::{contract::DEFAULT_UPDATE_PERIOD, state::INFO_OF_ICA_ID};
use crate::{
    contract::{msg_with_sudo_callback, SudoPayload, TxType, DEFAULT_TIMEOUT_SECONDS},
    state::ADDR_DELEGATIONS_REPLY_ID,
};
use crate::{error_conversion::ContractError, state::REPLY_ID_TO_QUERY_ID};
use crate::{helper::min_ntrn_ibc_fee, state::POOL_VALIDATOR_STATUS};

pub fn execute_pool_update_validator(
    mut deps: DepsMut<NeutronQuery>,
    _env: Env,
    info: MessageInfo,
    pool_addr: String,
    old_validator: String,
    new_validator: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.as_ref().storage, pool_addr.clone())?;

    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_pool_update_validator pool_info: {:?}",
            pool_info
        )
        .as_str(),
    );

    let mut pool_validator_status = POOL_VALIDATOR_STATUS.load(deps.storage, pool_addr.clone())?;
    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_pool_update_validator pool_validator_status: {:?}",
            pool_validator_status
        )
        .as_str(),
    );
    if pool_validator_status.status == ValidatorUpdateStatus::Pending {
        return Err(NeutronError::Std(StdError::generic_err("status not allow")));
    }

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
            "WASMDEBUG: execute_pool_update_validator delegations: {:?}",
            delegations
        )
        .as_str(),
    );

    pool_info
        .validator_addrs
        .retain(|x| x.as_str() != old_validator);
    pool_info.validator_addrs.push(new_validator.clone());

    let mut msgs = vec![];

    for delegation in delegations.delegations {
        if delegation.validator != old_validator {
            continue;
        }
        let stake_amount = delegation.amount.amount;

        if stake_amount.is_zero() {
            continue;
        }

        let any_msg = gen_redelegate_txs(
            pool_addr.clone(),
            delegation.validator.clone(),
            new_validator.clone(),
            pool_info.remote_denom.clone(),
            stake_amount,
        );

        msgs.push(any_msg);
    }
    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

    let register_delegation_query_msg = new_register_delegator_delegations_query_msg(
        pool_ica_info.ctrl_connection_id.clone(),
        pool_addr.clone(),
        pool_info.validator_addrs.clone(),
        DEFAULT_UPDATE_PERIOD,
    )?;

    let next_icq_reply_id = get_next_icq_reply_id(deps.storage, QueryKind::Delegations)?;
    let register_delegation_query_submsg =
        SubMsg::reply_on_success(register_delegation_query_msg, next_icq_reply_id);

    // let remove_msg_old_query = NeutronMsg::remove_interchain_query(registere_query_id);
    let mut resp = Response::default().add_submessage(register_delegation_query_submsg); // .add_message(remove_msg_old_query)

    if !msgs.is_empty() {
        let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
        let cosmos_msg = NeutronMsg::submit_tx(
            pool_ica_info.ctrl_connection_id.clone(),
            pool_info.ica_id.clone(),
            msgs,
            "".to_string(),
            DEFAULT_TIMEOUT_SECONDS,
            fee.clone(),
        );

        let new_validator_list_str = pool_info
            .validator_addrs
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

        resp = resp.add_submessage(submsg_redelegate)
    } else {
        pool_validator_status.status = ValidatorUpdateStatus::Success;
    }

    POOL_VALIDATOR_STATUS.save(deps.storage, pool_addr.clone(), &pool_validator_status)?;

    Ok(resp)
}

pub fn sudo_update_validators_callback(deps: DepsMut, payload: SudoPayload) -> StdResult<Response> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;
    let mut pool_validator_status =
        POOL_VALIDATOR_STATUS.load(deps.storage, payload.pool_addr.clone())?;

    let new_validators: Vec<String> = payload.message.split('_').map(String::from).collect();

    pool_info.validator_addrs = new_validators;
    pool_validator_status.status = ValidatorUpdateStatus::Success;

    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;
    POOL_VALIDATOR_STATUS.save(
        deps.storage,
        payload.pool_addr.clone(),
        &pool_validator_status,
    )?;

    Ok(Response::new())
}

pub fn sudo_update_validators_failed_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> StdResult<Response> {
    let mut pool_validator_status =
        POOL_VALIDATOR_STATUS.load(deps.storage, payload.pool_addr.clone())?;

    pool_validator_status.status = ValidatorUpdateStatus::Failed;

    POOL_VALIDATOR_STATUS.save(deps.storage, payload.pool_addr, &pool_validator_status)?;
    Ok(Response::new())
}

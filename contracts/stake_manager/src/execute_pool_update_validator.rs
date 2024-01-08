use crate::helper::min_ntrn_ibc_fee;
use crate::state::INFO_OF_ICA_ID;
use crate::state::{ValidatorUpdateStatus, POOLS};
use crate::{contract::DEFAULT_TIMEOUT_SECONDS, query::query_delegation_by_addr};
use crate::{error_conversion::ContractError, state::EraProcessStatus};
use crate::{
    helper::gen_redelegate_txs,
    state::{SudoPayload, TxType},
    tx_callback::msg_with_sudo_callback,
};
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdError, StdResult};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronError, NeutronResult,
};
use std::vec;

pub fn execute_pool_update_validator(
    mut deps: DepsMut<NeutronQuery>,
    _env: Env,
    info: MessageInfo,
    pool_addr: String,
    old_validator: String,
    new_validator: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info: crate::state::PoolInfo =
        POOLS.load(deps.as_ref().storage, pool_addr.clone())?;

    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }
    if pool_info.era_process_status != EraProcessStatus::ActiveEnded {
        return Err(NeutronError::Std(StdError::generic_err(
            "Era process not end",
        )));
    }

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_pool_update_validator pool_info: {:?}",
            pool_info
        )
        .as_str(),
    );

    if pool_info.validator_update_status == ValidatorUpdateStatus::Pending {
        return Err(NeutronError::Std(StdError::generic_err("status not allow")));
    }

    let delegations = query_delegation_by_addr(deps.as_ref(), pool_addr.clone())?;
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

        pool_info.validator_update_status = ValidatorUpdateStatus::Pending;

        resp = resp.add_submessage(submsg_redelegate)
    } else {
        pool_info.validator_update_status = ValidatorUpdateStatus::WaitQueryUpdate;
    }

    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;

    Ok(resp)
}

pub fn sudo_update_validators_callback(deps: DepsMut, payload: SudoPayload) -> StdResult<Response> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;

    let new_validators: Vec<String> = payload.message.split('_').map(String::from).collect();

    pool_info.validator_addrs = new_validators;
    pool_info.validator_update_status = ValidatorUpdateStatus::WaitQueryUpdate;

    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;

    Ok(Response::new())
}

pub fn sudo_update_validators_failed_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> StdResult<Response> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;

    pool_info.validator_update_status = ValidatorUpdateStatus::Failed;

    POOLS.save(deps.storage, payload.pool_addr, &pool_info)?;

    Ok(Response::new())
}

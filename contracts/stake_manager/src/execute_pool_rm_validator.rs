use crate::error_conversion::ContractError;
use crate::helper::gen_redelegate_txs;
use crate::helper::min_ntrn_ibc_fee;
use crate::helper::DEFAULT_TIMEOUT_SECONDS;
use crate::query::query_delegation_by_addr;
use crate::state::{
    EraStatus, SudoPayload, TxType, ValidatorUpdateStatus, INFO_OF_ICA_ID, POOLS,
};
use crate::tx_callback::msg_with_sudo_callback;
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronResult,
};
use std::vec;

pub fn execute_rm_pool_validator(
    mut deps: DepsMut<NeutronQuery>,
    _: Env,
    info: MessageInfo,
    pool_addr: String,
    validator_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.as_ref().storage, pool_addr.clone())?;

    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }
    if pool_info.status != EraStatus::ActiveEnded {
        return Err(ContractError::EraProcessNotEnd {}.into());
    }
    if !pool_info.validator_addrs.contains(&validator_addr) {
        return Err(ContractError::OldValidatorNotExist {}.into());
    }
    if pool_info.validator_update_status != ValidatorUpdateStatus::End {
        return Err(ContractError::StatusNotAllow {}.into());
    }

    let delegations = query_delegation_by_addr(deps.as_ref(), pool_addr.clone())?;

    if pool_info.validator_addrs.len() <= 1 {
        return Err(ContractError::ValidatorAddressesListSize {}.into());
    }

    let left_validators: Vec<String> = pool_info
        .validator_addrs
        .clone()
        .into_iter()
        .filter(|val| val.to_string() != validator_addr)
        .collect();
    let mut rsp = Response::new();
    if let Some(to_be_redelegate_delegation) = delegations
        .delegations
        .iter()
        .find(|d| d.validator == validator_addr)
    {
        if to_be_redelegate_delegation.amount.amount.is_zero() {
            pool_info.validator_addrs = left_validators;
        } else {
            let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
            let (pool_ica_info, _, _) =
                INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

            let cosmos_msg = NeutronMsg::submit_tx(
                pool_ica_info.ctrl_connection_id.clone(),
                pool_info.ica_id.clone(),
                vec![gen_redelegate_txs(
                    pool_addr.clone(),
                    to_be_redelegate_delegation.validator.clone(),
                    left_validators.get(0).unwrap().to_string(), // redelegate to first
                    pool_info.remote_denom.clone(),
                    to_be_redelegate_delegation.amount.amount,
                )],
                "".to_string(),
                DEFAULT_TIMEOUT_SECONDS,
                fee,
            );

            let submsg_redelegate = msg_with_sudo_callback(
                deps.branch(),
                cosmos_msg,
                SudoPayload {
                    port_id: pool_ica_info.ctrl_port_id,
                    pool_addr: pool_ica_info.ica_addr.clone(),
                    message: validator_addr,
                    tx_type: TxType::RmValidator,
                },
            )?;

            rsp = rsp.add_submessage(submsg_redelegate);
            pool_info.validator_update_status = ValidatorUpdateStatus::Start;
        }
    } else {
        pool_info.validator_addrs = left_validators;
    }

    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;

    Ok(rsp)
}

pub fn sudo_rm_validator_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;

    pool_info
        .validator_addrs
        .retain(|v| v.to_string() != payload.message);
    pool_info.validator_update_status = ValidatorUpdateStatus::WaitQueryUpdate;

    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;

    Ok(Response::new())
}

pub fn sudo_rm_validator_failed_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;

    pool_info.validator_update_status = ValidatorUpdateStatus::End;

    POOLS.save(deps.storage, payload.pool_addr, &pool_info)?;

    Ok(Response::new())
}

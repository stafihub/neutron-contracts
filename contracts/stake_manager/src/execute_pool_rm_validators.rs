use crate::contract::DEFAULT_TIMEOUT_SECONDS;
use crate::error_conversion::ContractError;
use crate::helper::gen_redelegate_txs;
use crate::helper::min_ntrn_ibc_fee;
use crate::query::query_delegation_by_addr;
use crate::state::{
    EraProcessStatus, SudoPayload, TxType, ValidatorUpdateStatus, INFO_OF_ICA_ID, POOLS,
};
use crate::tx_callback::msg_with_sudo_callback;
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdError};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronError, NeutronResult,
};
use std::vec;

pub fn execute_rm_pool_validators(
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
    if pool_info.era_process_status != EraProcessStatus::ActiveEnded {
        return Err(NeutronError::Std(StdError::generic_err(
            "Era process not end",
        )));
    }
    if !pool_info.validator_addrs.contains(&validator_addr) {
        return Err(NeutronError::Std(StdError::generic_err(
            "Validator not exist",
        )));
    }
    if pool_info.validator_update_status == ValidatorUpdateStatus::Pending {
        return Err(NeutronError::Std(StdError::generic_err("status not allow")));
    }

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_rm_pool_validators pool_info: {:?}",
            pool_info
        )
        .as_str(),
    );

    let delegations = query_delegation_by_addr(deps.as_ref(), pool_addr.clone())?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_rm_pool_validators delegations: {:?}",
            delegations
        )
        .as_str(),
    );

    pool_info
        .validator_addrs
        .retain(|val| val != &validator_addr);
    if pool_info.validator_addrs.len() == 0 {
        return Err(NeutronError::Std(StdError::generic_err(
            "validator_count is zero",
        )));
    }
    let mut rsp = Response::new();
    if let Some(to_be_redelegate_delegation) = delegations
        .delegations
        .iter()
        .find(|d| d.validator == validator_addr)
    {
        let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
        let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

        let cosmos_msg = NeutronMsg::submit_tx(
            pool_ica_info.ctrl_connection_id.clone(),
            pool_info.ica_id.clone(),
            vec![gen_redelegate_txs(
                pool_addr.clone(),
                to_be_redelegate_delegation.validator.clone(),
                pool_info.validator_addrs.get(0).unwrap().to_string(), // redelegate to first
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
                tx_type: TxType::UpdateValidators,
            },
        )?;

        rsp = rsp.add_submessage(submsg_redelegate);

        pool_info.validator_update_status = ValidatorUpdateStatus::Pending;
    }

    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;

    Ok(rsp)
}

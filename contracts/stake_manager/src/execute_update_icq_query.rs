use cosmwasm_std::{DepsMut, Env, MessageInfo, Response};

use neutron_sdk::interchain_queries::v045::{
    new_register_delegator_delegations_query_msg, new_register_staking_validators_query_msg,
};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

use crate::helper::DEFAULT_UPDATE_PERIOD;
use crate::state::{QueryKind, INFO_OF_ICA_ID};
use crate::state::{ValidatorUpdateStatus, POOLS};
use crate::{error_conversion::ContractError, helper::get_query_id};

pub fn execute_update_validators_icq(
    deps: DepsMut<NeutronQuery>,
    _env: Env,
    info: MessageInfo,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info: crate::state::PoolInfo =
        POOLS.load(deps.as_ref().storage, pool_addr.clone())?;

    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }

    if pool_info.validator_update_status != ValidatorUpdateStatus::WaitQueryUpdate {
        return Err(ContractError::StatusNotAllow {}.into());
    }
    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

    let new_delegations_keys = match new_register_delegator_delegations_query_msg(
        pool_ica_info.ctrl_connection_id.clone(),
        pool_addr.clone(),
        pool_info.validator_addrs.clone(),
        DEFAULT_UPDATE_PERIOD,
    ) {
        Ok(NeutronMsg::RegisterInterchainQuery { keys, .. }) => keys,
        _ => return Err(ContractError::ICQNewKeyBuildFailed {}.into()),
    };

    let update_pool_delegations_msg = NeutronMsg::update_interchain_query(
        get_query_id(deps.as_ref(), pool_addr.clone(), QueryKind::Delegations)?,
        Some(new_delegations_keys),
        None,
        None,
    )?;

    let new_validators_keys = match new_register_staking_validators_query_msg(
        pool_ica_info.ctrl_connection_id,
        pool_info.validator_addrs.clone(),
        DEFAULT_UPDATE_PERIOD,
    ) {
        Ok(NeutronMsg::RegisterInterchainQuery { keys, .. }) => keys,
        _ => return Err(ContractError::ICQNewKeyBuildFailed {}.into()),
    };

    let update_pool_validators_msg = NeutronMsg::update_interchain_query(
        get_query_id(deps.as_ref(), pool_addr.clone(), QueryKind::Validators)?,
        Some(new_validators_keys),
        None,
        None,
    )?;

    pool_info.validator_update_status = ValidatorUpdateStatus::End;
    POOLS.save(deps.storage, pool_addr, &pool_info)?;

    Ok(Response::default().add_messages(vec![
        update_pool_delegations_msg,
        update_pool_validators_msg,
    ]))
}

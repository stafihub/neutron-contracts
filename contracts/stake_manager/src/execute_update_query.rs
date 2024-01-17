use crate::state::QueryKind;
use crate::state::{ValidatorUpdateStatus, POOLS};
use crate::{error_conversion::ContractError, helper::get_query_id};
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    interchain_queries::get_registered_query,
    NeutronResult,
};

pub fn execute_update_query(
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

    let pool_delegations_query_id =
        get_query_id(deps.as_ref(), pool_addr.clone(), QueryKind::Delegations)?;

    let pool_delegations_registered_query: neutron_sdk::bindings::query::QueryRegisteredQueryResponse =
        get_registered_query(deps.as_ref(), pool_delegations_query_id)?;

    let update_pool_delegations_msg = NeutronMsg::update_interchain_query(
        pool_delegations_query_id,
        Some(pool_delegations_registered_query.registered_query.keys),
        None,
        None,
    )?;

    let pool_validators_query_id =
        get_query_id(deps.as_ref(), pool_addr.clone(), QueryKind::Validators)?;

    let pool_validators_registered_query: neutron_sdk::bindings::query::QueryRegisteredQueryResponse =
        get_registered_query(deps.as_ref(), pool_validators_query_id)?;

    let update_pool_validators_msg = NeutronMsg::update_interchain_query(
        pool_validators_query_id,
        Some(pool_validators_registered_query.registered_query.keys),
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

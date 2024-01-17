use cosmwasm_std::{DepsMut, Env, MessageInfo, Response};

use neutron_sdk::interchain_queries::get_registered_query;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

use crate::error_conversion::ContractError;
use crate::state::{EraProcessStatus, POOLS};
use crate::{helper::get_query_id, state::QueryKind};

pub fn execute_add_pool_validators(
    deps: DepsMut<NeutronQuery>,
    _: Env,
    info: MessageInfo,
    pool_addr: String,
    validator_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }
    if pool_info.era_process_status != EraProcessStatus::ActiveEnded {
        return Err(ContractError::EraProcessNotEnd {}.into());
    }

    if pool_info.validator_addrs.len() >= 5 {
        return Err(ContractError::ValidatorAddressesListSize {}.into());
    }
    if pool_info.validator_addrs.contains(&validator_addr) {
        return Err(ContractError::ValidatorAlreadyExit {}.into());
    }
    pool_info.validator_addrs.push(validator_addr);

    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;

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

    Ok(Response::default().add_messages(vec![
        update_pool_delegations_msg,
        update_pool_validators_msg,
    ]))
}

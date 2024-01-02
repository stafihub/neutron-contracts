use cosmwasm_std::{DepsMut, Response, StdError, SubMsg};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};
use neutron_sdk::{
    interchain_queries::v045::new_register_delegator_delegations_query_msg, NeutronError,
};

use crate::contract::DEFAULT_UPDATE_PERIOD;
use crate::state::LATEST_DELEGATIONS_QUERY_ID;
use crate::state::POOLS;

pub fn execute_add_pool_validators(
    deps: DepsMut<NeutronQuery>,
    pool_addr: String,
    validator_addrs: Vec<String>,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.as_ref().storage, pool_addr.clone())?;

    if validator_addrs.len() + pool_info.validator_addrs.len() > 5 {
        return Err(NeutronError::Std(StdError::generic_err(
            "Validator addresses list must contain between 1 and 5 addresses.",
        )));
    }

    let mut result = validator_addrs.clone()
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    result.extend(pool_info.validator_addrs.into_iter());
    pool_info.validator_addrs = result.into_iter().collect();

    let latest_delegation_query_id = LATEST_DELEGATIONS_QUERY_ID.load(deps.as_ref().storage)?;
    let pool_delegation_query_id = latest_delegation_query_id + 1;

    let register_delegation_query_msg = new_register_delegator_delegations_query_msg(
        pool_info.connection_id.clone(),
        pool_addr.clone(),
        pool_info.validator_addrs,
        DEFAULT_UPDATE_PERIOD,
    )?;

    let register_delegation_query_submsg =
        SubMsg::reply_on_success(register_delegation_query_msg, pool_delegation_query_id);

    LATEST_DELEGATIONS_QUERY_ID.save(deps.storage, &(latest_delegation_query_id + 1))?;

    Ok(Response::default().add_submessage(register_delegation_query_submsg))
}

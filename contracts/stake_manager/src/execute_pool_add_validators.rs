use cosmwasm_std::{DepsMut, Response, SubMsg};
use neutron_sdk::interchain_queries::v045::new_register_delegator_delegations_query_msg;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

use crate::contract::DEFAULT_UPDATE_PERIOD;
use crate::state::LATEST_DELEGATIONS_QUERY_ID;
use crate::state::POOLS;

pub fn execute_add_pool_validators(
    deps: DepsMut<NeutronQuery>,
    pool_addr: String,
    validator_addrs: Vec<String>,
) -> NeutronResult<Response<NeutronMsg>> {
    let pool_info = POOLS.load(deps.as_ref().storage, pool_addr.clone())?;

    let latest_delegation_query_id = LATEST_DELEGATIONS_QUERY_ID.load(deps.as_ref().storage)?;
    let pool_delegation_query_id = latest_delegation_query_id + 1;

    let register_delegation_query_msg = new_register_delegator_delegations_query_msg(
        pool_info.connection_id.clone(),
        pool_addr.clone(),
        validator_addrs,
        DEFAULT_UPDATE_PERIOD,
    )?;

    // wrap into submessage to save {query_id, query_type} on reply that'll later be used to handle sudo kv callback
    let register_delegation_query_submsg =
        SubMsg::reply_on_success(register_delegation_query_msg, pool_delegation_query_id);

    LATEST_DELEGATIONS_QUERY_ID.save(deps.storage, &(latest_delegation_query_id + 1))?;

    // todo update pool_info in query replay
    Ok(Response::default().add_submessage(register_delegation_query_submsg))
}

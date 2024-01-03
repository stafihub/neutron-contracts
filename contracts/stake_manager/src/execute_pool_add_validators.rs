use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdError, SubMsg};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};
use neutron_sdk::{
    interchain_queries::v045::new_register_delegator_delegations_query_msg, NeutronError,
};

use crate::state::POOLS;
use crate::state::{
    get_next_icq_reply_id, QueryKind, ADDR_DELEGATIONS_REPLY_ID, REPLY_ID_TO_QUERY_ID,
};
use crate::{contract::DEFAULT_UPDATE_PERIOD, error_conversion::ContractError};

pub fn execute_add_pool_validators(
    deps: DepsMut<NeutronQuery>,
    _: Env,
    info: MessageInfo,
    pool_addr: String,
    validator_addrs: Vec<String>,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.as_ref().storage, pool_addr.clone())?;

    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }

    if validator_addrs.len() + pool_info.validator_addrs.len() > 5 {
        return Err(NeutronError::Std(StdError::generic_err(
            "Validator addresses list must contain between 1 and 5 addresses.",
        )));
    }

    let mut result = validator_addrs
        .clone()
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    result.extend(pool_info.validator_addrs.clone().into_iter());
    pool_info.validator_addrs = result.into_iter().collect();

    let contract_query_id = ADDR_DELEGATIONS_REPLY_ID.load(deps.storage, pool_addr.clone())?;
    let registered_query_id = REPLY_ID_TO_QUERY_ID.load(deps.storage, contract_query_id)?;
    let remove_icq_msg = NeutronMsg::remove_interchain_query(registered_query_id);

    // register new query
    let register_delegation_query_msg = new_register_delegator_delegations_query_msg(
        pool_info.connection_id.clone(),
        pool_addr.clone(),
        pool_info.validator_addrs.clone(),
        DEFAULT_UPDATE_PERIOD,
    )?;

    let next_icq_reply_id = get_next_icq_reply_id(deps.storage, QueryKind::Delegations)?;

    let register_delegation_query_submsg =
        SubMsg::reply_on_success(register_delegation_query_msg, next_icq_reply_id);

    ADDR_DELEGATIONS_REPLY_ID.save(deps.storage, pool_addr.clone(), &next_icq_reply_id)?;
    POOLS.save(deps.storage, pool_addr, &pool_info)?;

    Ok(Response::new()
        .add_message(remove_icq_msg)
        .add_submessage(register_delegation_query_submsg))
}

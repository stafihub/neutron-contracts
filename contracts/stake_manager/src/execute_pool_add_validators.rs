use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdError, SubMsg};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    interchain_queries::v045::new_register_staking_validators_query_msg,
    NeutronResult,
};
use neutron_sdk::{
    interchain_queries::v045::new_register_delegator_delegations_query_msg, NeutronError,
};

use crate::state::{ADDR_VALIDATOR_REPLY_ID, POOLS};

use crate::state::{get_next_icq_reply_id, QueryKind, ADDR_DELEGATIONS_REPLY_ID, INFO_OF_ICA_ID};

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

    // todo: remove old query wait for test in testnet Testing the network locally would cause ICQ to be completely unavailable
    // let contract_query_id = ADDR_DELEGATIONS_REPLY_ID.load(deps.storage, pool_addr.clone())?;
    // let registered_query_id = REPLY_ID_TO_QUERY_ID.load(deps.storage, contract_query_id)?;
    // let remove_icq_msg = NeutronMsg::remove_interchain_query(registered_query_id);

    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;
    // register new query
    let register_delegation_query_msg = new_register_delegator_delegations_query_msg(
        pool_ica_info.ctrl_connection_id.clone(),
        pool_addr.clone(),
        pool_info.validator_addrs.clone(),
        DEFAULT_UPDATE_PERIOD,
    )?;

    let register_staking_validators_query_msg = new_register_staking_validators_query_msg(
        pool_ica_info.ctrl_connection_id.clone(),
        pool_info.validator_addrs.clone(),
        DEFAULT_UPDATE_PERIOD,
    )?;

    let next_icq_reply_id_for_delegations =
        get_next_icq_reply_id(deps.storage, QueryKind::Delegations)?;
    let next_icq_reply_id_for_validator =
        get_next_icq_reply_id(deps.storage, QueryKind::Validator)?;

    let register_delegation_query_submsg = SubMsg::reply_on_success(
        register_delegation_query_msg,
        next_icq_reply_id_for_delegations,
    );
    let register_staking_validator_query_submsg = SubMsg::reply_on_success(
        register_staking_validators_query_msg,
        next_icq_reply_id_for_validator,
    );

    ADDR_DELEGATIONS_REPLY_ID.save(
        deps.storage,
        pool_addr.clone(),
        &next_icq_reply_id_for_delegations,
    )?;
    ADDR_VALIDATOR_REPLY_ID.save(
        deps.storage,
        pool_addr.clone(),
        &next_icq_reply_id_for_validator,
    )?;

    POOLS.save(deps.storage, pool_addr, &pool_info)?;

    Ok(Response::new()
        // .add_message(remove_icq_msg)
        .add_submessage(register_delegation_query_submsg)
        .add_submessage(register_staking_validator_query_submsg))
}

use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdError};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    interchain_queries::v045::new_register_staking_validators_query_msg,
    NeutronResult,
};
use neutron_sdk::{
    interchain_queries::v045::new_register_delegator_delegations_query_msg, NeutronError,
};

use crate::{
    query_callback::register_query_submsg,
    state::{EraProcessStatus, POOLS},
};

use crate::state::{QueryKind, INFO_OF_ICA_ID};

use crate::{contract::DEFAULT_UPDATE_PERIOD, error_conversion::ContractError};

pub fn execute_add_pool_validators(
    mut deps: DepsMut<NeutronQuery>,
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
        return Err(NeutronError::Std(StdError::generic_err(
            "Era process not end",
        )));
    }

    if pool_info.validator_addrs.len() >= 5 {
        return Err(NeutronError::Std(StdError::generic_err(
            "Validator addresses list must contain between 1 and 5 addresses.",
        )));
    }
    if pool_info.validator_addrs.contains(&validator_addr) {
        return Err(NeutronError::Std(StdError::generic_err(
            "validator already exit",
        )));
    }
    pool_info.validator_addrs.push(validator_addr);

    // todo: remove old query wait for test in testnet Testing the network locally would cause ICQ to be completely unavailable
    // let old_reply_id_delegations = ADDRESS_TO_REPLY_ID.load(
    //     deps.storage,
    //     (pool_addr.clone(), QueryKind::Delegations.to_string()),
    // )?;
    // let need_rm_query_id_delegations =
    //     REPLY_ID_TO_QUERY_ID.load(deps.storage, old_reply_id_delegations)?;
    // let remove_icq_msg_delegations =
    //     NeutronMsg::remove_interchain_query(need_rm_query_id_delegations);

    // let old_reply_id_validators = ADDRESS_TO_REPLY_ID.load(
    //     deps.storage,
    //     (pool_addr.clone(), QueryKind::Validators.to_string()),
    // )?;
    // let need_rm_query_id_validators =
    //     REPLY_ID_TO_QUERY_ID.load(deps.storage, old_reply_id_validators)?;
    // let remove_icq_msg_validators =
    //     NeutronMsg::remove_interchain_query(need_rm_query_id_validators);

    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

    let register_delegation_submsg = register_query_submsg(
        deps.branch(),
        new_register_delegator_delegations_query_msg(
            pool_ica_info.ctrl_connection_id.clone(),
            pool_ica_info.ica_addr.clone(),
            pool_info.validator_addrs.clone(),
            DEFAULT_UPDATE_PERIOD,
        )?,
        pool_ica_info.ica_addr.clone(),
        QueryKind::Delegations,
    )?;

    let register_validator_submsg = register_query_submsg(
        deps.branch(),
        new_register_staking_validators_query_msg(
            pool_ica_info.ctrl_connection_id.clone(),
            pool_info.validator_addrs.clone(),
            DEFAULT_UPDATE_PERIOD,
        )?,
        pool_ica_info.ica_addr.clone(),
        QueryKind::Validators,
    )?;

    POOLS.save(deps.storage, pool_addr, &pool_info)?;

    Ok(Response::new()
        // .add_message(remove_icq_msg_delegations)
        // .add_message(remove_icq_msg_validators)
        .add_submessage(register_delegation_submsg)
        .add_submessage(register_validator_submsg))
}

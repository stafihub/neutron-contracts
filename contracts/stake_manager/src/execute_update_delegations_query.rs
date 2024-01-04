use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdError, SubMsg};

use neutron_sdk::interchain_queries::v045::new_register_delegator_delegations_query_msg;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronError, NeutronResult,
};

use crate::error_conversion::ContractError;
use crate::state::POOL_VALIDATOR_STATUS;
use crate::state::{get_next_icq_reply_id, QueryKind, ValidatorUpdateStatus, POOLS};
use crate::{contract::DEFAULT_UPDATE_PERIOD, state::INFO_OF_ICA_ID};

pub fn execute_update_delegations_query(
    deps: DepsMut<NeutronQuery>,
    _env: Env,
    info: MessageInfo,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let pool_info: crate::state::PoolInfo = POOLS.load(deps.as_ref().storage, pool_addr.clone())?;

    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }

    let mut pool_validator_status = POOL_VALIDATOR_STATUS.load(deps.storage, pool_addr.clone())?;
    if pool_validator_status.status != ValidatorUpdateStatus::WaitQueryUpdate {
        return Err(NeutronError::Std(StdError::generic_err("status not allow")));
    }

    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

    let register_delegation_query_msg = new_register_delegator_delegations_query_msg(
        pool_ica_info.ctrl_connection_id.clone(),
        pool_addr.clone(),
        pool_info.validator_addrs.clone(),
        DEFAULT_UPDATE_PERIOD,
    )?;

    let next_icq_reply_id = get_next_icq_reply_id(deps.storage, QueryKind::Delegations)?;
    let register_delegation_query_submsg =
        SubMsg::reply_on_success(register_delegation_query_msg, next_icq_reply_id);

    pool_validator_status.status = ValidatorUpdateStatus::Success;
    POOL_VALIDATOR_STATUS.save(deps.storage, pool_addr.clone(), &pool_validator_status)?;

    Ok(Response::default().add_submessage(register_delegation_query_submsg))
}

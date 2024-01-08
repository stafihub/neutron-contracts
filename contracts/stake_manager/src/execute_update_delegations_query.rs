use crate::error_conversion::ContractError;
use crate::query_callback::register_query_submsg;
use crate::state::QueryKind;
use crate::state::{ValidatorUpdateStatus, POOLS};
use crate::{contract::DEFAULT_UPDATE_PERIOD, state::INFO_OF_ICA_ID};
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdError};
use neutron_sdk::interchain_queries::v045::new_register_delegator_delegations_query_msg;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronError, NeutronResult,
};

pub fn execute_update_delegations_query(
    mut deps: DepsMut<NeutronQuery>,
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
        return Err(NeutronError::Std(StdError::generic_err("status not allow")));
    }

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

    pool_info.validator_update_status = ValidatorUpdateStatus::Success;

    POOLS.save(deps.storage, pool_addr, &pool_info)?;

    Ok(Response::default().add_submessage(register_delegation_submsg))
}

use cosmwasm_std::{DepsMut, MessageInfo, Response};

use neutron_sdk::bindings::query::NeutronQuery;
use neutron_sdk::{bindings::msg::NeutronMsg, NeutronResult};

use crate::contract::{DEFAULT_FAST_PERIOD, DEFAULT_UPDATE_PERIOD};
use crate::error_conversion::ContractError;
use crate::helper::get_update_pool_icq_msgs;
use crate::state::EraProcessStatus::ActiveEnded;
use crate::state::{ValidatorUpdateStatus, POOLS};

pub fn update_icq_update_period(
    deps: DepsMut<NeutronQuery>,
    info: MessageInfo,
    pool_addr: String,
    new_update_period: u64,
) -> NeutronResult<Response<NeutronMsg>> {
    if new_update_period < DEFAULT_FAST_PERIOD {
        return Err(ContractError::PeriodTooSmall {}.into());
    }

    let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    if pool_info.admin != info.sender {
        return Err(ContractError::Unauthorized {}.into());
    }

    // check era state
    if pool_info.era_process_status != ActiveEnded
        || pool_info.validator_update_status != ValidatorUpdateStatus::End
    {
        return Err(ContractError::StatusNotAllow {}.into());
    }

    let update_pool_icq_msgs = get_update_pool_icq_msgs(
        deps,
        pool_addr,
        pool_info.ica_id.clone(),
        DEFAULT_UPDATE_PERIOD,
    )?;

    Ok(Response::default().add_messages(update_pool_icq_msgs))
}

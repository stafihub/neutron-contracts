use std::ops::{Add, Div};

use cosmwasm_std::{DepsMut, Env, Response};

use neutron_sdk::bindings::msg::NeutronMsg;
use neutron_sdk::bindings::query::NeutronQuery;
use neutron_sdk::NeutronResult;

use crate::contract::DEFAULT_FAST_PERIOD;
use crate::error_conversion::ContractError;
use crate::helper::get_update_pool_icq_msgs;
use crate::state::EraProcessStatus::{ActiveEnded, EraPreprocessEnded};
use crate::state::{ValidatorUpdateStatus, POOLS};

pub fn execute_era_preprocess(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    if pool_info.paused {
        return Err(ContractError::PoolIsPaused {}.into());
    }
    // check era state
    if pool_info.era_process_status != ActiveEnded
        && pool_info.validator_update_status != ValidatorUpdateStatus::End
    {
        return Err(ContractError::StatusNotAllow {}.into());
    }
    let current_era = env
        .block
        .time
        .seconds()
        .div(pool_info.era_seconds)
        .add(pool_info.offset);

    if current_era <= pool_info.era {
        return Err(ContractError::AlreadyLatestEra {}.into());
    }

    pool_info.era_process_status = EraPreprocessEnded;
    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;

    let update_pool_icq_msgs = get_update_pool_icq_msgs(
        deps,
        pool_addr,
        pool_info.ica_id.clone(),
        DEFAULT_FAST_PERIOD,
    )?;

    Ok(Response::default().add_messages(update_pool_icq_msgs))
}

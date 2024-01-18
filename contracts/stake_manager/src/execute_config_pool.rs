use cosmwasm_std::{Addr, DepsMut, MessageInfo, Response};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

use crate::{error_conversion::ContractError, msg::ConfigPoolParams};
use crate::{helper::MIN_ERA_SECONDS, state::POOLS};

pub fn execute_config_pool(
    deps: DepsMut<NeutronQuery>,
    info: MessageInfo,
    param: ConfigPoolParams,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.as_ref().storage, param.pool_addr.clone())?;

    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }

    if let Some(minimal_stake) = param.minimal_stake {
        pool_info.minimal_stake = minimal_stake;
    }
    if let Some(unbonding_period) = param.unbonding_period {
        pool_info.unbonding_period = unbonding_period;
    }
    if let Some(unstake_times_limit) = param.unstake_times_limit {
        pool_info.unstake_times_limit = unstake_times_limit;
    }
    if let Some(unbond_commission) = param.unbond_commission {
        pool_info.unbond_commission = unbond_commission;
    }
    if let Some(platform_fee_commission) = param.platform_fee_commission {
        pool_info.platform_fee_commission = platform_fee_commission;
    }
    if let Some(era_seconds) = param.era_seconds {
        if era_seconds < MIN_ERA_SECONDS {
            return Err(ContractError::LessThanMinimalEraSeconds {}.into());
        }
        pool_info.era_seconds = era_seconds;
    }
    if let Some(offset) = param.offset {
        pool_info.offset = offset;
    }
    if let Some(receiver) = param.platform_fee_receiver {
        pool_info.platform_fee_receiver = Addr::unchecked(receiver);
    }
    if let Some(paused) = param.paused {
        pool_info.paused = paused;
    }
    if let Some(lsm_support) = param.lsm_support {
        pool_info.lsm_support = lsm_support;
    }
    if let Some(lsm_pending_limit) = param.lsm_pending_limit {
        pool_info.lsm_pending_limit = lsm_pending_limit;
    }
    if let Some(rate_change_limit) = param.rate_change_limit {
        pool_info.rate_change_limit = rate_change_limit;
    }
    if let Some(new_admin) = param.new_admin {
        pool_info.admin = new_admin;
    }

    POOLS.save(deps.storage, param.pool_addr.clone(), &pool_info)?;

    Ok(Response::default())
}

use cosmwasm_std::{Addr, DepsMut, MessageInfo, Response};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

use crate::state::POOLS;
use crate::{error_conversion::ContractError, msg::ConfigPoolParams};

pub fn execute_config_pool(
    deps: DepsMut<NeutronQuery>,
    info: MessageInfo,
    param: ConfigPoolParams,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.as_ref().storage, param.pool_addr.clone())?;

    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }

    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: execute_config_pool POOLS.load: {:?}", pool_info).as_str());

    if let Some(minimal_stake) = param.minimal_stake {
        pool_info.minimal_stake = minimal_stake;
    }
    if let Some(next_unstake_index) = param.next_unstake_index {
        pool_info.next_unstake_index = next_unstake_index;
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
    if let Some(protocol_fee_commission) = param.protocol_fee_commission {
        pool_info.protocol_fee_commission = protocol_fee_commission;
    }
    if let Some(era_seconds) = param.era_seconds {
        pool_info.era_seconds = era_seconds;
    }
    if let Some(offset) = param.offset {
        pool_info.offset = offset;
    }
    if let Some(receiver) = param.protocol_fee_receiver {
        pool_info.protocol_fee_receiver = Addr::unchecked(receiver);
    }
    if let Some(paused) = param.paused {
        pool_info.paused = paused;
    }

    POOLS.save(deps.storage, param.pool_addr.clone(), &pool_info)?;

    Ok(Response::default())
}

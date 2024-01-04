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

    pool_info.minimal_stake = param.minimal_stake;
    pool_info.rtoken = Addr::unchecked(param.rtoken);
    pool_info.next_unstake_index = param.next_unstake_index;
    pool_info.unbonding_period = param.unbonding_period;
    pool_info.unstake_times_limit = param.unstake_times_limit;
    pool_info.unbond_commission = param.unbond_commission;
    pool_info.protocol_fee_receiver = Addr::unchecked(param.protocol_fee_receiver);
    pool_info.era_seconds = param.era_seconds;
    pool_info.offset = param.offset;

    POOLS.save(deps.storage, param.pool_addr.clone(), &pool_info)?;

    Ok(Response::default())
}

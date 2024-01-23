use crate::helper::{
    deal_pool, set_withdraw_sub_msg, CAL_BASE, DEFAULT_ERA_SECONDS, MIN_ERA_SECONDS,
};
use crate::msg::InitPoolParams;
use crate::state::ValidatorUpdateStatus;
use crate::state::POOLS;
use crate::state::{INFO_OF_ICA_ID, STACK};
use crate::{error_conversion::ContractError, state::EraStatus};
use cosmwasm_std::{Addr, Uint128};
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};
use std::ops::Div;
use std::vec;

// add execute to config the validator addrs and withdraw address on reply
pub fn execute_init_pool(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    param: InitPoolParams,
) -> NeutronResult<Response<NeutronMsg>> {
    let (pool_ica_info, withdraw_ica_info, _) =
        INFO_OF_ICA_ID.load(deps.storage, param.interchain_account_id.clone())?;

    if param.validator_addrs.is_empty() || param.validator_addrs.len() > 5 {
        return Err(ContractError::ValidatorAddressesListSize {}.into());
    }

    let mut pool_info = POOLS.load(deps.as_ref().storage, pool_ica_info.ica_addr.clone())?;
    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }
    if pool_info.status == EraStatus::InitFailed {
        return Ok(Response::new().add_submessage(set_withdraw_sub_msg(
            deps,
            pool_info,
            pool_ica_info,
            withdraw_ica_info,
        )?));
    }

    if pool_info.status != EraStatus::RegisterEnded {
        return Err(ContractError::StatusNotAllow {}.into());
    }

    pool_info.ibc_denom = param.ibc_denom;
    pool_info.channel_id_of_ibc_denom = param.channel_id_of_ibc_denom;
    pool_info.remote_denom = param.remote_denom;
    pool_info.validator_addrs = param.validator_addrs.clone();
    pool_info.platform_fee_receiver = Addr::unchecked(param.platform_fee_receiver);
    pool_info.unbonding_period = param.unbonding_period;
    pool_info.minimal_stake = param.minimal_stake;

    // option
    if let Some(platform_fee_commission) = param.platform_fee_commission {
        pool_info.platform_fee_commission = platform_fee_commission;
    } else {
        pool_info.platform_fee_commission = Uint128::new(100_000);
    }

    if let Some(era_seconds) = param.era_seconds {
        if era_seconds < MIN_ERA_SECONDS {
            return Err(ContractError::LessThanMinimalEraSeconds {}.into());
        }
        pool_info.era_seconds = era_seconds;
    } else {
        pool_info.era_seconds = DEFAULT_ERA_SECONDS;
    }

    // cal
    let offset = env.block.time.seconds().div(pool_info.era_seconds);
    pool_info.offset = offset;

    // default
    pool_info.era = 0;
    pool_info.bond = Uint128::zero();
    pool_info.unbond = Uint128::zero();
    pool_info.active = Uint128::zero();
    pool_info.rate = CAL_BASE;
    pool_info.share_tokens = vec![];
    pool_info.total_platform_fee = Uint128::zero();
    pool_info.total_lsd_token_amount = Uint128::zero();
    pool_info.next_unstake_index = 0;
    pool_info.unstake_times_limit = 20;
    pool_info.unbond_commission = Uint128::zero();
    pool_info.paused = false;
    pool_info.lsm_support = false;
    pool_info.lsm_pending_limit = 100;
    pool_info.rate_change_limit = Uint128::zero();
    pool_info.validator_update_status = ValidatorUpdateStatus::End;

    let code_id = match param.lsd_code_id {
        Some(lsd_code_id) => lsd_code_id,
        None => STACK.load(deps.storage)?.lsd_token_code_id,
    };

    deal_pool(
        deps,
        env,
        info,
        pool_info,
        pool_ica_info,
        withdraw_ica_info,
        code_id,
        param.lsd_token_name,
        param.lsd_token_symbol,
    )
}

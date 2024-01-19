use crate::error_conversion::ContractError;
use crate::helper::deal_pool;
use crate::helper::CAL_BASE;
use crate::msg::MigratePoolParams;
use crate::state::ValidatorUpdateStatus;
use crate::state::POOLS;
use crate::state::{INFO_OF_ICA_ID, STACK};
use cosmwasm_std::{Addr, Uint128};
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};
use std::ops::{Div, Mul};

// add execute to config the validator addrs and withdraw address on reply
pub fn execute_migrate_pool(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    param: MigratePoolParams,
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
    if !pool_info.rate.is_zero() {
        return Err(ContractError::PoolInited {}.into());
    }
    if param.rate.is_zero() {
        return Err(ContractError::RateIsZero {}.into());
    }

    pool_info.bond = param.bond;
    pool_info.unbond = param.unbond;
    pool_info.active = param.active;
    pool_info.era = param.era;
    pool_info.rate = param.rate;
    pool_info.ibc_denom = param.ibc_denom;
    pool_info.channel_id_of_ibc_denom = param.channel_id_of_ibc_denom;
    pool_info.remote_denom = param.remote_denom;
    pool_info.validator_addrs = param.validator_addrs.clone();
    pool_info.platform_fee_receiver = Addr::unchecked(param.platform_fee_receiver);
    pool_info.share_tokens = param.share_tokens;
    pool_info.total_platform_fee = param.total_platform_fee;
    pool_info.total_lsd_token_amount = param.total_lsd_token_amount;
    pool_info.era_seconds = param.era_seconds;
    pool_info.offset = param.offset;
    pool_info.unbonding_period = param.unbonding_period;
    pool_info.minimal_stake = param.minimal_stake;

    // option
    if let Some(platform_fee_commission) = param.platform_fee_commission {
        pool_info.platform_fee_commission = platform_fee_commission;
    } else {
        pool_info.platform_fee_commission = Uint128::new(100_000);
    }

    // default
    pool_info.next_unstake_index = 0;
    pool_info.unstake_times_limit = 20;
    pool_info.unbond_commission = Uint128::zero();
    pool_info.paused = false;
    pool_info.lsm_support = true;
    pool_info.lsm_pending_limit = 50;
    pool_info.rate_change_limit = Uint128::zero();
    pool_info.validator_update_status = ValidatorUpdateStatus::End;

    // check rate
    let cal_rate = if pool_info.total_lsd_token_amount.is_zero() {
        CAL_BASE
    } else {
        pool_info
            .active
            .mul(CAL_BASE)
            .div(pool_info.total_lsd_token_amount)
    };
    if cal_rate != pool_info.rate {
        return Err(ContractError::RateNotMatch {}.into());
    }

    let code_id = match param.lsd_code_id {
        Some(lsd_code_id) => lsd_code_id,
        None => STACK.load(deps.storage)?.lsd_token_code_id,
    };
    return deal_pool(
        deps,
        env,
        info,
        pool_info,
        pool_ica_info,
        withdraw_ica_info,
        code_id,
        param.lsd_token_name,
        param.lsd_token_symbol,
    );
}

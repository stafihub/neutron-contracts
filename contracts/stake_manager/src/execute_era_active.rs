use core::ops::{Mul, Sub};
use std::ops::{Add, Div};

use cosmwasm_std::{to_json_binary, DepsMut, Response, Uint128, WasmMsg};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

use crate::helper::{DEFAULT_RATE, DEFAULT_UPDATE_PERIOD};
use crate::state::{
    EraStatus::{ActiveEnded, RebondEnded},
    STACK,
};
use crate::{error_conversion::ContractError, state::POOLS};
use crate::{helper::get_update_pool_icq_msgs, state::ERA_RATE};
use crate::{helper::CAL_BASE, query::query_delegation_by_addr};

pub fn execute_era_active(
    deps: DepsMut<NeutronQuery>,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    // check era state
    if pool_info.status != RebondEnded {
        return Err(ContractError::StatusNotAllow {}.into());
    }

    if pool_info.share_tokens.len() > 0 {
        return Err(ContractError::PendingShareNotEmpty {}.into());
    }

    let delegations_result = query_delegation_by_addr(deps.as_ref(), pool_addr.clone());

    let mut total_amount = cosmwasm_std::Coin {
        denom: pool_info.remote_denom.clone(),
        amount: Uint128::zero(),
    };

    match delegations_result {
        Ok(delegations_resp) => {
            if delegations_resp.last_submitted_local_height
                <= pool_info.era_snapshot.last_step_height
            {
                return Err(ContractError::DelegationSubmissionHeight {}.into());
            }
            for delegation in delegations_resp.delegations {
                total_amount.amount = total_amount.amount.add(delegation.amount.amount);
            }
        }
        Err(_) => {
            return Err(ContractError::DelegationsNotExist {}.into());
        }
    }

    let mut stack_info = STACK.load(deps.storage)?;
    // calculate protocol fee
    let (platform_fee, stack_fee) = if total_amount.amount > pool_info.era_snapshot.active {
        let reward = total_amount.amount.sub(pool_info.era_snapshot.active);
        let platform_fee_raw = reward
            .mul(pool_info.platform_fee_commission)
            .div(pool_info.rate);

        let stack_fee = platform_fee_raw
            .mul(stack_info.stack_fee_commission)
            .div(CAL_BASE);
        (platform_fee_raw.sub(stack_fee), stack_fee)
    } else {
        (Uint128::zero(), Uint128::zero())
    };

    let cal_temp = pool_info.active.add(total_amount.amount);
    let mut new_active = if cal_temp > pool_info.era_snapshot.active {
        cal_temp.sub(pool_info.era_snapshot.active)
    } else {
        Uint128::zero()
    };

    pool_info.total_lsd_token_amount = pool_info
        .total_lsd_token_amount
        .add(platform_fee)
        .add(stack_fee);
    let mut new_rate = if pool_info.total_lsd_token_amount.u128() > 0 {
        new_active
            .mul(CAL_BASE)
            .div(pool_info.total_lsd_token_amount)
    } else {
        CAL_BASE
    };

    if !pool_info.rate_change_limit.is_zero() {
        let rate_change = if pool_info.rate > new_rate {
            pool_info
                .rate
                .sub(new_rate)
                .mul(CAL_BASE)
                .div(pool_info.rate)
        } else {
            new_rate
                .sub(pool_info.rate)
                .mul(CAL_BASE)
                .div(pool_info.rate)
        };

        if rate_change > pool_info.rate_change_limit {
            return Err(ContractError::RateChangeOverLimit {}.into());
        }
    }

    // Solve first stake calculation accuracy
    if pool_info.rate == DEFAULT_RATE
        && new_rate < DEFAULT_RATE
        && pool_info.active < new_active.add(Uint128::new(1000))
    {
        new_rate = DEFAULT_RATE;
        new_active = pool_info.active;
    }

    pool_info.rate = new_rate;
    pool_info.status = ActiveEnded;
    pool_info.bond = Uint128::zero();
    pool_info.unbond = Uint128::zero();
    pool_info.active = new_active;

    let mut resp = Response::new().add_attribute("new_rate", pool_info.rate);
    if !platform_fee.is_zero() {
        let msg = WasmMsg::Execute {
            contract_addr: pool_info.lsd_token.to_string(),
            msg: to_json_binary(
                &(lsd_token::msg::ExecuteMsg::Mint {
                    recipient: pool_info.platform_fee_receiver.to_string(),
                    amount: platform_fee,
                }),
            )?,
            funds: vec![],
        };
        resp = resp.add_message(msg);

        pool_info.total_platform_fee = pool_info.total_platform_fee.add(platform_fee);
    }
    if !stack_fee.is_zero() {
        let msg = WasmMsg::Execute {
            contract_addr: pool_info.lsd_token.to_string(),
            msg: to_json_binary(
                &(lsd_token::msg::ExecuteMsg::Mint {
                    recipient: stack_info.stack_fee_receiver.to_string(),
                    amount: stack_fee,
                }),
            )?,
            funds: vec![],
        };
        resp = resp.add_message(msg);

        stack_info.total_stack_fee = stack_info.total_stack_fee.add(stack_fee);
    }

    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;
    STACK.save(deps.storage, &stack_info)?;
    ERA_RATE.save(
        deps.storage,
        (pool_addr.clone(), pool_info.era),
        &pool_info.rate,
    )?;

    let update_pool_icq_msgs = get_update_pool_icq_msgs(
        deps,
        pool_addr.clone(),
        pool_info.ica_id.clone(),
        DEFAULT_UPDATE_PERIOD,
    )?;

    Ok(resp
        .add_messages(update_pool_icq_msgs)
        .add_attribute("action", "era_active")
        .add_attribute("pool", pool_addr)
        .add_attribute("era", pool_info.era.to_string())
        .add_attribute("rate", new_rate))
}

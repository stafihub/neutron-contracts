use core::ops::{Mul, Sub};
use std::ops::{Add, Div};

use cosmwasm_std::{
    to_json_binary, DepsMut, QueryRequest, Response, StdError, Uint128, WasmMsg, WasmQuery,
};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronError, NeutronResult,
};

use crate::state::POOLS;
use crate::state::{
    EraProcessStatus::{ActiveEnded, RestakeEnded},
    PLATFORM_INFO,
};
use crate::{helper::CAL_BASE, query::query_delegation_by_addr};

pub fn execute_era_active(
    deps: DepsMut<NeutronQuery>,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    // check era state
    if pool_info.era_process_status != RestakeEnded {
        deps.as_ref()
            .api
            .debug(format!("WASMDEBUG: execute_era_active skip pool: {:?}", pool_addr).as_str());
        return Err(NeutronError::Std(StdError::generic_err("status not allow")));
    }

    if pool_info.pending_share_tokens.len() > 0 {
        return Err(NeutronError::Std(StdError::generic_err(
            "Pending share token not empty",
        )));
    }

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_era_active pool_era_shot: {:?}",
            pool_info.era_snapshot
        )
        .as_str(),
    );

    let delegations_result = query_delegation_by_addr(deps.as_ref(), pool_addr.clone());

    let mut total_amount = cosmwasm_std::Coin {
        denom: pool_info.remote_denom.clone(),
        amount: Uint128::zero(),
    };

    match delegations_result {
        Ok(delegations_resp) => {
            if delegations_resp.last_submitted_local_height <= pool_info.era_snapshot.bond_height {
                return Err(NeutronError::Std(StdError::generic_err("Delegation submission height is less than 
                or equal to the bond/withdraw collect height of the pool era, which is not allowed.")));
            }
            for delegation in delegations_resp.delegations {
                total_amount.amount = total_amount.amount.add(delegation.amount.amount);
            }
        }
        Err(_) => {
            deps.as_ref().api.debug(
                format!(
                    "WASMDEBUG: execute_era_active delegations_result: {:?}",
                    delegations_result
                )
                .as_str(),
            );

            return Err(NeutronError::Std(StdError::generic_err(
                "delegations not exist",
            )));
        }
    }

    let token_info_msg = rtoken::msg::QueryMsg::TokenInfo {};
    let token_info: cw20::TokenInfoResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: pool_info.lsd_token.to_string(),
            msg: to_json_binary(&token_info_msg)?,
        }))?;

    let platform_info = PLATFORM_INFO.load(deps.storage)?;
    // calculate protocol fee
    let (protocol_fee, platform_fee) = if total_amount.amount > pool_info.era_snapshot.active {
        let reward = total_amount.amount.sub(pool_info.era_snapshot.active);
        let protocol_fee_raw = reward
            .mul(pool_info.protocol_fee_commission)
            .div(pool_info.rate);

        let platform_fee = protocol_fee_raw
            .mul(platform_info.platform_fee_commission)
            .div(CAL_BASE);
        (protocol_fee_raw.sub(platform_fee), platform_fee)
    } else {
        (Uint128::zero(), Uint128::zero())
    };

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_era_active protocol_fee is: {:?}, total_amount is: {:?}, token_info is: {:?}",
            protocol_fee,total_amount,token_info
        )
        .as_str(),
    );

    let cal_temp = pool_info.active.add(total_amount.amount);
    let new_active = if cal_temp > pool_info.era_snapshot.active {
        cal_temp.sub(pool_info.era_snapshot.active)
    } else {
        Uint128::zero()
    };

    let total_rtoken_amount = token_info.total_supply.add(protocol_fee).add(platform_fee);
    let new_rate = if total_rtoken_amount.u128() > 0 {
        new_active.mul(CAL_BASE).div(total_rtoken_amount)
    } else {
        CAL_BASE
    };

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
        return Err(NeutronError::Std(StdError::generic_err(
            "rate change over limit",
        )));
    }

    pool_info.rate = new_rate;
    pool_info.era_process_status = ActiveEnded;
    pool_info.bond = Uint128::zero();
    pool_info.unbond = Uint128::zero();
    pool_info.active = new_active;

    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;

    let mut resp = Response::new().add_attribute("new_rate", pool_info.rate);
    if !protocol_fee.is_zero() {
        let msg = WasmMsg::Execute {
            contract_addr: pool_info.lsd_token.to_string(),
            msg: to_json_binary(
                &(rtoken::msg::ExecuteMsg::Mint {
                    recipient: pool_info.protocol_fee_receiver.to_string(),
                    amount: protocol_fee,
                }),
            )?,
            funds: vec![],
        };
        resp = resp.add_message(msg);
    }
    if !platform_fee.is_zero() {
        let msg = WasmMsg::Execute {
            contract_addr: pool_info.lsd_token.to_string(),
            msg: to_json_binary(
                &(rtoken::msg::ExecuteMsg::Mint {
                    recipient: platform_info.platform_fee_receiver.to_string(),
                    amount: platform_fee,
                }),
            )?,
            funds: vec![],
        };
        resp = resp.add_message(msg);
    }

    Ok(resp)
}

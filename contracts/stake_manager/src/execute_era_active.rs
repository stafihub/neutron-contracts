use core::ops::{Mul, Sub};
use std::ops::{Add, Div};

use cosmwasm_std::{
    to_json_binary, DepsMut, Env, QueryRequest, Response, StdError, Uint128, WasmMsg, WasmQuery,
};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronError, NeutronResult,
};

use crate::state::PoolBondState;
use crate::state::PoolBondState::WithdrawReported;
use crate::state::POOLS;
use crate::{query::query_delegation_by_addr, state::POOL_ERA_SHOT};

pub fn execute_era_active(
    deps: DepsMut<NeutronQuery>,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    // check era state
    if pool_info.era_update_status != WithdrawReported {
        deps.as_ref()
            .api
            .debug(format!("WASMDEBUG: execute_era_active skip pool: {:?}", pool_addr).as_str());
        return Err(NeutronError::Std(StdError::generic_err("status not allow")));
    }

    let pool_era_shot = POOL_ERA_SHOT.load(deps.storage, pool_addr.clone())?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_era_active pool_era_shot: {:?}",
            pool_era_shot
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
            if delegations_resp.last_submitted_local_height <= pool_era_shot.bond_height {
                return Err(NeutronError::Std(StdError::generic_err("Delegation submission height is less than or equal to the bond height of the pool era, which is not allowed.")));
            }
            for delegation in delegations_resp.delegations {
                total_amount.amount = total_amount.amount.add(delegation.amount.amount);
            }
        }
        Err(_) => {
            // return Err(NeutronError::Std(StdError::generic_err(
            //     "balance not exist",
            // )));
            deps.as_ref().api.debug(
                format!(
                    "WASMDEBUG: execute_era_active delegations_result: {:?}",
                    delegations_result
                )
                .as_str(),
            );
        }
    }

    let token_info_msg = rtoken::msg::QueryMsg::TokenInfo {};
    let token_info: cw20::TokenInfoResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: pool_info.rtoken.to_string(),
            msg: to_json_binary(&token_info_msg)?,
        }))?;

    // calculate protocol fee
    let protocol_fee = if pool_info.active > pool_era_shot.active {
        let reward = pool_info.active.sub(pool_era_shot.active);
        reward.mul(pool_info.rate).div(Uint128::new(1_000_000))
    } else {
        Uint128::zero()
    };

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_era_active protocol_fee is: {:?}",
            protocol_fee
        )
        .as_str(),
    );

    pool_info.rate = total_amount
        .amount
        .div(token_info.total_supply.add(protocol_fee));
    pool_info.era_update_status = PoolBondState::ActiveReported;
    pool_info.bond = Uint128::zero();
    pool_info.unbond = Uint128::zero();
    pool_info.era += 1;
    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;
    POOL_ERA_SHOT.remove(deps.storage, pool_addr);

    let mut resp = Response::new().add_attribute("new_rate", pool_info.rate);
    if !protocol_fee.is_zero() {
        let msg = WasmMsg::Execute {
            contract_addr: pool_info.rtoken.to_string(),
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

    Ok(resp)
}

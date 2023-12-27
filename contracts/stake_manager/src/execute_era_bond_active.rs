use std::ops::{Add, Div};

use cosmwasm_std::{to_json_binary, DepsMut, Env, QueryRequest, Response, Uint128, WasmQuery};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

use crate::query::query_delegation_by_addr;
use crate::state::PoolBondState;
use crate::state::PoolBondState::BondReported;
use crate::state::POOLS;

pub fn execute_bond_active(
    deps: DepsMut<NeutronQuery>,
    _: Env,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    // check era state
    if pool_info.era_update_status != BondReported {
        deps.as_ref()
            .api
            .debug(format!("WASMDEBUG: execute_era_bond skip pool: {:?}", pool_addr).as_str());
        return Ok(Response::default());
    }

    let delegations = query_delegation_by_addr(deps.as_ref(), pool_addr.clone())?;

    let mut total_amount = cosmwasm_std::Coin {
        denom: pool_info.remote_denom.clone(),
        amount: Uint128::zero(),
    };

    for delegation in delegations.delegations {
        total_amount.amount = total_amount.amount.add(delegation.amount.amount);
    }

    let token_info_msg = rtoken::msg::QueryMsg::TokenInfo {};
    let token_info: cw20::TokenInfoResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: pool_info.rtoken.to_string(),
            msg: to_json_binary(&token_info_msg)?,
        }))?;
    // todo: calculate protocol fee
    pool_info.rate = total_amount.amount.div(token_info.total_supply);
    pool_info.era_update_status = PoolBondState::ActiveReported;
    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;

    Ok(Response::default())
}

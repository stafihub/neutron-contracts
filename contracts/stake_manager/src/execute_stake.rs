use std::ops::{Add, Div, Mul};
use std::vec;

use cosmwasm_std::{
    to_json_binary, CosmosMsg, DepsMut, Env, MessageInfo, Response, Uint128, WasmMsg,
};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

use crate::state::POOLS;

pub fn execute_stake(
    deps: DepsMut<NeutronQuery>,
    _: Env,
    neutron_address: String,
    pool_addr: String,
    info: MessageInfo,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    let mut token_amount = 0;
    if !info.funds.is_empty() {
        token_amount = u128::from(
            info.funds
                .iter()
                .find(|c| c.denom == pool_info.ibc_denom.clone())
                .map(|c| c.amount)
                .unwrap_or(Uint128::zero()),
        );
    }

    pool_info.active = pool_info.active.add(Uint128::new(token_amount));
    pool_info.bond = pool_info.active.add(Uint128::new(token_amount));

    let rtoken_amount = token_amount.mul(pool_info.rate.u128()).div(1_000_000);

    let msg = WasmMsg::Execute {
        contract_addr: pool_info.rtoken.to_string(),
        msg: to_json_binary(
            &(rtoken::msg::ExecuteMsg::Mint {
                recipient: neutron_address.to_string(),
                amount: Uint128::from(rtoken_amount),
            }),
        )?,
        funds: vec![],
    };

    POOLS.save(deps.storage, pool_addr, &pool_info)?;

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(msg))
        .add_attribute("mint", "call_contract_b"))
}
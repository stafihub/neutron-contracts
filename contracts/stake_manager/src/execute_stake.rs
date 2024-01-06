use std::ops::{Add, Div, Mul};
use std::vec;

use cosmwasm_std::{
    to_json_binary, CosmosMsg, DepsMut, Env, MessageInfo, Response, StdError, Uint128, WasmMsg,
};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronError, NeutronResult,
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
    if pool_info.paused {
        return Err(NeutronError::Std(StdError::generic_err("Pool is paused")));
    }

    if info.funds.len() != 1 || info.funds[0].denom != pool_info.ibc_denom.clone() {
        return Err(NeutronError::Std(StdError::generic_err(format!(
            "Params error: {}",
            "funds not match"
        ))));
    }

    let token_amount = info.funds[0].amount;
    if token_amount < pool_info.minimal_stake {
        return Err(NeutronError::Std(StdError::generic_err(format!(
            "Params error: {}",
            "less than minimal stake"
        ))));
    }

    pool_info.active = pool_info.active.add(token_amount);
    pool_info.bond = pool_info.bond.add(token_amount);

    let rtoken_amount = token_amount
        .mul(Uint128::new(1_000_000))
        .div(pool_info.rate);

    let msg = WasmMsg::Execute {
        contract_addr: pool_info.rtoken.to_string(),
        msg: to_json_binary(
            &(rtoken::msg::ExecuteMsg::Mint {
                recipient: neutron_address.to_string(),
                amount: rtoken_amount,
            }),
        )?,
        funds: vec![],
    };

    POOLS.save(deps.storage, pool_addr, &pool_info)?;

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(msg))
        .add_attribute("mint", rtoken_amount.to_string()))
}

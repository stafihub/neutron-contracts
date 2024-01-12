use std::ops::{Add, Div, Mul};
use std::vec;

use cosmwasm_std::{
    to_json_binary, CosmosMsg, DepsMut, Env, MessageInfo, Response, StdError, WasmMsg,
};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronError, NeutronResult,
};

use crate::helper::CAL_BASE;
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

    let lsd_token_amount = token_amount.mul(CAL_BASE).div(pool_info.rate);

    let msg = WasmMsg::Execute {
        contract_addr: pool_info.lsd_token.to_string(),
        msg: to_json_binary(
            &(lsd_token::msg::ExecuteMsg::Mint {
                recipient: neutron_address.to_string(),
                amount: lsd_token_amount,
            }),
        )?,
        funds: vec![],
    };
    pool_info.total_lsd_token_amount = pool_info.total_lsd_token_amount.add(lsd_token_amount);

    POOLS.save(deps.storage, pool_addr, &pool_info)?;

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(msg))
        .add_attribute("mint", lsd_token_amount.to_string()))
}

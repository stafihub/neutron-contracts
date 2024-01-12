use std::ops::{Add, Div, Mul, Sub};
use std::vec;

use cosmwasm_std::{
    to_json_binary, CosmosMsg, DepsMut, MessageInfo, Response, StdError, Uint128, WasmMsg,
};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronError, NeutronResult,
};

use crate::helper::CAL_BASE;
use crate::state::{
    UnstakeInfo, WithdrawStatus, POOLS, UNSTAKES_INDEX_FOR_USER, UNSTAKES_OF_INDEX,
};

// Before this step, need the user to authorize burn from
pub fn execute_unstake(
    deps: DepsMut<NeutronQuery>,
    info: MessageInfo,
    lsd_token_amount: Uint128,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    if lsd_token_amount == Uint128::zero() {
        return Err(NeutronError::Std(StdError::generic_err(format!(
            "Encode error: {}",
            "lsd_token amount is zero"
        ))));
    }

    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: execute_unstake pool_info: {:?}", pool_info).as_str());

    let mut unstakes_index_for_user = UNSTAKES_INDEX_FOR_USER
        .load(deps.storage, (info.sender.clone(), pool_addr.clone()))
        .unwrap_or_else(|_| vec![]);

    let unstake_count = unstakes_index_for_user.len() as u64;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_unstake UNSTAKES_INDEX_FOR_USER: {:?}",
            unstake_count
        )
        .as_str(),
    );

    let unstake_limit = pool_info.unstake_times_limit;
    if unstake_count >= unstake_limit {
        return Err(NeutronError::Std(StdError::generic_err(format!(
            "Encode error: {}",
            "Unstake times limit reached"
        ))));
    }

    let mut rsp = Response::new();
    // cal fee
    let mut will_burn_lsd_token_amount = lsd_token_amount;
    if pool_info.unbond_commission > Uint128::zero() {
        let cms_fee = lsd_token_amount
            .mul(pool_info.unbond_commission)
            .div(CAL_BASE);
        will_burn_lsd_token_amount = lsd_token_amount.sub(cms_fee);

        if cms_fee.u128() > 0 {
            let mint_msg = WasmMsg::Execute {
                contract_addr: pool_info.lsd_token.to_string(),
                msg: to_json_binary(
                    &(lsd_token::msg::ExecuteMsg::TransferFrom {
                        owner: info.sender.to_string(),
                        recipient: pool_info.platform_fee_receiver.to_string(),
                        amount: cms_fee,
                    }),
                )?,
                funds: vec![],
            };

            rsp = rsp.add_message(mint_msg);
        }

        deps.as_ref().api.debug(
            format!(
                "WASMDEBUG: execute_unstake cms_fee: {:?} lsd_token_amount: {:?}",
                cms_fee, lsd_token_amount
            )
            .as_str(),
        );
    }
    if will_burn_lsd_token_amount.is_zero() {
        return Err(NeutronError::Std(StdError::generic_err(
            "Burn lsd_token amount is zero",
        )));
    }

    // Calculate the number of tokens(atom)
    let token_amount = will_burn_lsd_token_amount.mul(pool_info.rate).div(CAL_BASE);

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_unstake token_amount: {:?}",
            token_amount
        )
        .as_str(),
    );

    // update pool info
    pool_info.next_unstake_index += 1;
    pool_info.unbond = pool_info.unbond.add(token_amount);
    pool_info.active = pool_info.active.sub(token_amount);

    // burn
    let burn_msg = WasmMsg::Execute {
        contract_addr: pool_info.lsd_token.to_string(),
        msg: to_json_binary(
            &(lsd_token::msg::ExecuteMsg::BurnFrom {
                owner: info.sender.to_string(),
                amount: will_burn_lsd_token_amount,
            }),
        )?,
        funds: vec![],
    };
    pool_info.total_lsd_token_amount = pool_info
        .total_lsd_token_amount
        .sub(will_burn_lsd_token_amount);

    // update unstake info
    let will_use_unstake_index = pool_info.next_unstake_index;
    let unstake_info = UnstakeInfo {
        era: pool_info.era,
        pool_addr: pool_addr.clone(),
        unstaker: info.sender.to_string(),
        amount: token_amount,
        status: WithdrawStatus::Default,
    };

    unstakes_index_for_user.push(will_use_unstake_index);

    UNSTAKES_OF_INDEX.save(
        deps.storage,
        (pool_addr.clone(), will_use_unstake_index),
        &unstake_info,
    )?;
    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;
    UNSTAKES_INDEX_FOR_USER.save(
        deps.storage,
        (info.sender.clone(), pool_addr.clone()),
        &unstakes_index_for_user,
    )?;

    // send event
    Ok(rsp
        .add_message(CosmosMsg::Wasm(burn_msg))
        .add_attribute("action", "unstake")
        .add_attribute("from", info.sender.to_string())
        .add_attribute("token_amount", token_amount.to_string())
        .add_attribute("lsd_token_amount", lsd_token_amount.to_string())
        .add_attribute("unstake_index", will_use_unstake_index.to_string()))
}

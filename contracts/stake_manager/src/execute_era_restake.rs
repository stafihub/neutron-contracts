use std::ops::{Div, Mul, Sub};

use cosmwasm_std::{DepsMut, Env, Response, StdError, StdResult, Uint128};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    interchain_txs::helpers::get_port_id,
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronError, NeutronResult,
};

use crate::helper::{gen_delegation_txs, min_ntrn_ibc_fee};
use crate::state::EraProcessStatus::{RestakeEnded, RestakeStarted, WithdrawEnded};
use crate::state::POOLS;
use crate::{contract::DEFAULT_TIMEOUT_SECONDS, state::POOL_ERA_SHOT};
use crate::{
    contract::{msg_with_sudo_callback, SudoPayload, TxType},
    state::ADDR_ICAID_MAP,
};

pub fn execute_era_restake(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    // check era state
    if pool_info.era_process_status != WithdrawEnded {
        deps.as_ref()
            .api
            .debug(format!("WASMDEBUG: execute_era_restake skip pool: {:?}", pool_addr).as_str());
        return Err(NeutronError::Std(StdError::generic_err("status not allow")));
    }
    pool_info.era_process_status = RestakeStarted;

    let pool_era_shot = POOL_ERA_SHOT.load(deps.storage, pool_addr.clone())?;

    if env.block.height <= pool_era_shot.bond_height {
        return Err(NeutronError::Std(StdError::generic_err("Pool Addr submission height is less than or equal to the bond height of the pool era, which is not allowed.")));
    }

    let restake_amount = pool_era_shot.restake_amount;

    // leave gas
    if restake_amount.is_zero() {
        pool_info.era_process_status = RestakeEnded;
        POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;
        return Ok(Response::default());
    }

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_era_restake restake_amount: {:?}",
            restake_amount
        )
        .as_str(),
    );

    let interchain_account_id = ADDR_ICAID_MAP.load(deps.storage, pool_addr.clone())?;

    let validator_count = pool_info.validator_addrs.len() as u128;

    let mut msgs = vec![];
    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_era_restake stake_amount: {}, validator_count is {}",
            restake_amount, validator_count
        )
        .as_str(),
    );

    if validator_count == 0 {
        return Err(NeutronError::Std(StdError::generic_err(
            "validator_count is zero",
        )));
    }

    let amount_per_validator = restake_amount.div(Uint128::from(validator_count));
    let remainder = restake_amount.sub(amount_per_validator.mul(Uint128::new(validator_count)));

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_era_restake amount_per_validator: {}, remainder is {}",
            amount_per_validator, remainder
        )
        .as_str(),
    );

    for (index, validator_addr) in pool_info.validator_addrs.iter().enumerate() {
        let mut amount_for_this_validator = amount_per_validator;

        // Add the remainder to the first validator
        if index == 0 {
            amount_for_this_validator += remainder;
        }

        deps.as_ref().api.debug(
            format!(
                "WASMDEBUG: execute_era_restake Validator: {}, Bond: {}",
                validator_addr, amount_for_this_validator
            )
            .as_str(),
        );

        let any_msg = gen_delegation_txs(
            pool_addr.clone(),
            validator_addr.clone(),
            pool_info.remote_denom.clone(),
            amount_for_this_validator,
        );

        msgs.push(any_msg);
    }

    let cosmos_msg = NeutronMsg::submit_tx(
        pool_info.connection_id.clone(),
        interchain_account_id.clone(),
        msgs,
        "".to_string(),
        DEFAULT_TIMEOUT_SECONDS,
        min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee),
    );

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_era_restake cosmos_msg: {:?}",
            cosmos_msg
        )
        .as_str(),
    );

    let submsg = msg_with_sudo_callback(
        deps.branch(),
        cosmos_msg,
        SudoPayload {
            port_id: get_port_id(
                env.contract.address.to_string(),
                interchain_account_id.clone(),
            ),
            // the acknowledgement later
            message: "".to_string(),
            pool_addr: pool_addr.clone(),
            tx_type: TxType::EraRestake,
        },
    )?;

    Ok(Response::default().add_submessage(submsg))
}

pub fn sudo_era_restake_callback(
    deps: DepsMut,
    env: Env,
    payload: SudoPayload,
) -> StdResult<Response> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;
    pool_info.era_process_status = RestakeEnded;
    POOLS.save(deps.storage, pool_info.pool_addr.clone(), &pool_info)?;

    let mut pool_era_shot = POOL_ERA_SHOT.load(deps.storage, payload.pool_addr.clone())?;
    pool_era_shot.bond_height = env.block.height;
    POOL_ERA_SHOT.save(deps.storage, payload.pool_addr, &pool_era_shot)?;

    Ok(Response::new())
}

pub fn sudo_era_restake_failed_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> StdResult<Response> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;
    pool_info.era_process_status = WithdrawEnded;
    POOLS.save(deps.storage, pool_info.pool_addr.clone(), &pool_info)?;

    Ok(Response::new())
}

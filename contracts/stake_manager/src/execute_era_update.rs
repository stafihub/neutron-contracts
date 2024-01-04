use std::ops::{Add, Div, Sub};

use cosmwasm_std::{coin, DepsMut, Env, Response, StdError, StdResult, Uint128};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    sudo::msg::RequestPacketTimeoutHeight,
    NeutronError, NeutronResult,
};

use crate::helper::min_ntrn_ibc_fee;
use crate::state::EraProcessStatus::{ActiveEnded, EraUpdateEnded, EraUpdateStarted};
use crate::state::{INFO_OF_ICA_ID, POOLS};
use crate::{
    contract::{msg_with_sudo_callback, SudoPayload, TxType},
    state::{EraShot, POOL_ERA_SHOT},
};

pub fn execute_era_update(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    if pool_info.paused {
        return Err(NeutronError::Std(StdError::generic_err("Pool is paused")));
    }
    // check era state
    if pool_info.era_process_status != ActiveEnded {
        deps.as_ref()
            .api
            .debug(format!("WASMDEBUG: execute_era_update skip pool: {:?}", pool_addr).as_str());
        return Err(NeutronError::Std(StdError::generic_err("status not allow")));
    }
    let current_era = env
        .block
        .time
        .seconds()
        .div(pool_info.era_seconds)
        .add(pool_info.offset);

    if current_era <= pool_info.era {
        return Err(NeutronError::Std(StdError::generic_err(
            "already latest era",
        )));
    }

    pool_info.era_process_status = EraUpdateStarted;
    pool_info.era = pool_info.era.add(1);

    POOL_ERA_SHOT.save(
        deps.storage,
        pool_addr.clone(),
        &(EraShot {
            era: pool_info.era,
            bond: pool_info.bond,
            unbond: pool_info.unbond,
            active: pool_info.active,
            bond_height: 0,
            restake_amount: Uint128::zero(),
        }),
    )?;

    if pool_info.bond.is_zero() {
        pool_info.era_process_status = EraUpdateEnded;
        POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;
        return Ok(Response::default());
    }

    // funds use contract funds
    let balance = deps.querier.query_all_balances(&env.contract.address)?;
    let mut amount = 0;
    if !balance.is_empty() {
        amount = u128::from(
            balance
                .iter()
                .find(|c| c.denom == pool_info.ibc_denom.clone())
                .map(|c| c.amount)
                .unwrap_or(Uint128::zero()),
        );
    }

    if amount == 0 {
        pool_info.era_process_status = EraUpdateEnded;
        POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;
        return Ok(Response::default());
    }

    let tx_coin = coin(amount, pool_info.ibc_denom.clone());
    // See more info here: https://docs.neutron.org/neutron/feerefunder/overview
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let msg: NeutronMsg = NeutronMsg::IbcTransfer {
        source_port: "transfer".to_string(),
        source_channel: pool_info.channel_id_of_ibc_denom,
        sender: env.contract.address.to_string(),
        receiver: pool_addr.clone(),
        token: tx_coin,
        timeout_height: RequestPacketTimeoutHeight {
            // todo: revision_number from param?
            revision_number: Some(2),
            revision_height: Some(crate::contract::DEFAULT_TIMEOUT_HEIGHT),
        },
        timeout_timestamp: 0,
        memo: "".to_string(),
        fee: fee.clone(),
    };

    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: IbcTransfer msg: {:?}", msg).as_str());

    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id)?;

    let submsg_pool_ibc_send = msg_with_sudo_callback(
        deps.branch(),
        msg,
        SudoPayload {
            port_id: pool_ica_info.ctrl_port_id,
            pool_addr: pool_addr.clone(),
            message: "".to_string(),
            tx_type: TxType::EraUpdate,
        },
    )?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_send: sent submsg: {:?}",
            submsg_pool_ibc_send
        )
        .as_str(),
    );

    Ok(Response::default().add_submessage(submsg_pool_ibc_send))
}

pub fn sudo_era_update_callback(deps: DepsMut, payload: SudoPayload) -> StdResult<Response> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;
    pool_info.era_process_status = EraUpdateEnded;
    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;

    Ok(Response::new())
}

pub fn sudo_era_update_failed_callback(deps: DepsMut, payload: SudoPayload) -> StdResult<Response> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;
    pool_info.era = pool_info.era.sub(1);
    pool_info.era_process_status = ActiveEnded;
    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;

    POOL_ERA_SHOT.remove(deps.storage, payload.pool_addr);

    Ok(Response::new())
}

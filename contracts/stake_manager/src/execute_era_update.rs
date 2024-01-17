use std::ops::{Add, Div, Sub};

use cosmwasm_std::{coin, DepsMut, Env, Response, Uint128};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    sudo::msg::RequestPacketTimeoutHeight,
    NeutronResult,
};

use crate::state::EraProcessStatus::EraPreprocessEnded;
use crate::state::{INFO_OF_ICA_ID, POOLS};
use crate::{contract::DEFAULT_TIMEOUT_SECONDS, state::EraSnapshot};
use crate::{
    error_conversion::ContractError,
    state::{
        EraProcessStatus::{ActiveEnded, EraUpdateEnded, EraUpdateStarted},
        ValidatorUpdateStatus,
    },
};
use crate::{
    helper::min_ntrn_ibc_fee,
    state::{SudoPayload, TxType},
    tx_callback::msg_with_sudo_callback,
};

pub fn execute_era_update(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    if pool_info.paused {
        return Err(ContractError::PoolIsPaused {}.into());
    }
    // check era state
    if pool_info.era_process_status != EraPreprocessEnded
        || pool_info.validator_update_status != ValidatorUpdateStatus::End
    {
        deps.as_ref()
            .api
            .debug(format!("WASMDEBUG: execute_era_update skip pool: {:?}", pool_addr).as_str());
        return Err(ContractError::StatusNotAllow {}.into());
    }
    let current_era = env
        .block
        .time
        .seconds()
        .div(pool_info.era_seconds)
        .sub(pool_info.offset);

    if current_era <= pool_info.era {
        return Err(ContractError::AlreadyLatestEra {}.into());
    }

    pool_info.era_process_status = EraUpdateStarted;
    pool_info.era = pool_info.era.add(1);
    pool_info.era_snapshot = EraSnapshot {
        era: pool_info.era,
        bond: pool_info.bond,
        unbond: pool_info.unbond,
        active: pool_info.active,
        bond_height: 0,
        restake_amount: Uint128::zero(),
    };

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
        source_channel: pool_info.channel_id_of_ibc_denom.clone(),
        sender: env.contract.address.to_string(),
        receiver: pool_addr.clone(),
        token: tx_coin,
        timeout_height: RequestPacketTimeoutHeight {
            revision_number: None,
            revision_height: None,
        },
        timeout_timestamp: env.block.time.nanos() + DEFAULT_TIMEOUT_SECONDS * 1_000_000_000,
        memo: "".to_string(),
        fee: fee.clone(),
    };

    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: IbcTransfer msg: {:?}", msg).as_str());

    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

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
    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;
    Ok(Response::default().add_submessage(submsg_pool_ibc_send))
}

pub fn sudo_era_update_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    deps.as_ref()
        .api
        .debug("WASMDEBUG: sudo_era_update_callback");

    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;
    pool_info.era_process_status = EraUpdateEnded;
    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;

    Ok(Response::new())
}

pub fn sudo_era_update_failed_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    deps.as_ref()
        .api
        .debug("WASMDEBUG: sudo_era_update_failed_callback");

    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;
    pool_info.era = pool_info.era.sub(1);
    pool_info.era_process_status = ActiveEnded;
    pool_info.era_snapshot = EraSnapshot {
        era: 0,
        bond: Uint128::zero(),
        unbond: Uint128::zero(),
        active: Uint128::zero(),
        restake_amount: Uint128::zero(),
        bond_height: 0,
    };

    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;

    Ok(Response::new())
}

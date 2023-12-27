use std::ops::Add;
use std::vec;

use cosmwasm_std::{coin, DepsMut, Env, Order, Response, Uint128};
use neutron_sdk::interchain_txs::helpers::get_port_id;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    interchain_queries::v045::types::Balances,
    query::min_ibc_fee::query_min_ibc_fee,
    sudo::msg::RequestPacketTimeoutHeight,
    NeutronResult,
};

use crate::contract::{
    msg_with_sudo_callback, SudoPayload, TxType, DEFAULT_TIMEOUT_HEIGHT, DEFAULT_TIMEOUT_SECONDS,
};
use crate::query::query_balance_by_addr;
use crate::state::PoolBondState::{ActiveReported, EraUpdated};
use crate::state::{POOLS, POOL_ICA_MAP, UNSTAKES_OF_INDEX};

pub fn execute_era_update(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    channel: String,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let unstaks = UNSTAKES_OF_INDEX.range(deps.storage, None, None, Order::Ascending);
    let mut need_withdraw = Uint128::zero();
    for unstake in unstaks {
        let (_, unstake_info) = unstake?;
        need_withdraw = need_withdraw.add(unstake_info.amount);
    }

    // --------------------------------------------------------------------------------------------------
    // contract must pay for relaying of acknowledgements
    // See more info here: https://docs.neutron.org/neutron/feerefunder/overview
    let fee = crate::contract::min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let mut msgs = vec![];
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    // check era state
    if pool_info.era_update_status != ActiveReported {
        deps.as_ref()
            .api
            .debug(format!("WASMDEBUG: execute_era_update skip pool: {:?}", pool_addr).as_str());
        return Ok(Response::new());
    }

    let balance = deps.querier.query_all_balances(&env.contract.address)?;

    // funds use contract funds
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

    let tx_coin = coin(amount, pool_info.ibc_denom.clone());

    let msg = NeutronMsg::IbcTransfer {
        source_port: "transfer".to_string(),
        source_channel: channel.clone(),
        sender: env.contract.address.to_string(),
        receiver: pool_addr.clone(),
        token: tx_coin,
        timeout_height: RequestPacketTimeoutHeight {
            // todo: revision_number to pool_info?
            revision_number: Some(2),
            revision_height: Some(crate::contract::DEFAULT_TIMEOUT_HEIGHT),
        },
        timeout_timestamp: DEFAULT_TIMEOUT_SECONDS,
        memo: "".to_string(),
        fee: fee.clone(),
    };

    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: IbcTransfer msg: {:?}", msg).as_str());

    let interchain_account_id = POOL_ICA_MAP.load(deps.storage, pool_addr.clone())?;

    let submsg_pool_ibc_send = msg_with_sudo_callback(
        deps.branch(),
        msg,
        SudoPayload {
            port_id: get_port_id(
                env.contract.address.to_string(),
                interchain_account_id.clone(),
            ),
            message: "era_update_ibc_token_send".to_string(),
            tx_type: TxType::EraUpdateIbcSend,
        },
    )?;
    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_send: sent submsg: {:?}",
            submsg_pool_ibc_send
        )
        .as_str(),
    );
    msgs.push(submsg_pool_ibc_send);

    // check withdraw address balance and send it to the pool
    let withdraw_balances: Balances =
        query_balance_by_addr(deps.as_ref(), pool_info.withdraw_addr.clone())?.balances;

    let mut withdraw_amount = 0;
    if !withdraw_balances.coins.is_empty() {
        withdraw_amount = u128::from(
            balance
                .iter()
                .find(|c| c.denom == pool_info.ibc_denom.clone())
                .map(|c| c.amount)
                .unwrap_or(Uint128::zero()),
        );
    }

    pool_info.era_update_status = EraUpdated;
    pool_info.need_withdraw = need_withdraw;
    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;
    if withdraw_amount == 0 {
        return Ok(Response::default());
    }

    // todo: Check whether the delegator-validator needs to manually withdraw
    let tx_withdraw_coin = coin(withdraw_amount, pool_info.ibc_denom.clone());
    let withdraw_token_send = NeutronMsg::IbcTransfer {
        source_port: "transfer".to_string(),
        source_channel: channel.clone(),
        sender: env.contract.address.to_string(),
        receiver: pool_addr.clone(),
        token: tx_withdraw_coin,
        timeout_height: RequestPacketTimeoutHeight {
            // todo: revision_number to pool_info?
            revision_number: Some(2),
            revision_height: Some(DEFAULT_TIMEOUT_HEIGHT),
        },
        timeout_timestamp: DEFAULT_TIMEOUT_SECONDS,
        memo: "".to_string(),
        fee: fee.clone(),
    };

    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: IbcTransfer msg: {:?}", withdraw_token_send).as_str());

    let submsg_withdraw_ibc_send = msg_with_sudo_callback(
        deps.branch(),
        withdraw_token_send,
        SudoPayload {
            port_id: get_port_id(env.contract.address.to_string(), interchain_account_id),
            message: "era_update_withdraw_token_send".to_string(),
            tx_type: TxType::EraUpdateWithdrawSend,
        },
    )?;
    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_send: sent submsg: {:?}",
            submsg_withdraw_ibc_send
        )
        .as_str(),
    );
    msgs.push(submsg_withdraw_ibc_send);

    Ok(Response::default().add_submessages(msgs))
}

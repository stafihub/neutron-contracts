use std::ops::Add;

use cosmwasm_std::{ coin, DepsMut, Env, Order, Response, Uint128, StdResult };
use neutron_sdk::interchain_txs::helpers::get_port_id;
use neutron_sdk::{
    bindings::{ msg::NeutronMsg, query::NeutronQuery },
    query::min_ibc_fee::query_min_ibc_fee,
    sudo::msg::RequestPacketTimeoutHeight,
    NeutronResult,
};

use crate::{
    contract::{ msg_with_sudo_callback, SudoPayload, TxType, DEFAULT_TIMEOUT_SECONDS },
    state::{ WithdrawStatus, POOL_ERA_SHOT, EraShot },
};
use crate::state::PoolBondState::{ EraUpdated, ActiveReported };
use crate::state::{ POOLS, POOL_ICA_MAP, UNSTAKES_OF_INDEX };

pub fn execute_era_update(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    channel: String,
    pool_addr: String
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    // check era state
    if pool_info.era_update_status != ActiveReported {
        deps.as_ref().api.debug(
            format!("WASMDEBUG: execute_era_update skip pool: {:?}", pool_addr).as_str()
        );
        return Ok(Response::new());
    }

    if let Some(pool_era_shot) = POOL_ERA_SHOT.may_load(deps.storage, pool_addr.clone())? {
        if
            pool_era_shot.failed_tx.is_some() &&
            pool_era_shot.failed_tx != Some(TxType::EraUpdateIbcSend)
        {
            return Ok(Response::new());
        }
    } else {
        POOL_ERA_SHOT.save(
            deps.storage,
            pool_addr.clone(),
            &(EraShot {
                pool_addr: pool_addr.clone(),
                era: pool_info.era,
                bond: Uint128::zero(),
                unbond: pool_info.unbond,
                active: pool_info.active,
                failed_tx: None,
            })
        )?;
    }

    // See more info here: https://docs.neutron.org/neutron/feerefunder/overview
    let fee = crate::contract::min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);

    let balance = deps.querier.query_all_balances(&env.contract.address)?;

    // funds use contract funds
    let mut amount = 0;
    if !balance.is_empty() {
        amount = u128::from(
            balance
                .iter()
                .find(|c| c.denom == pool_info.ibc_denom.clone())
                .map(|c| c.amount)
                .unwrap_or(Uint128::zero())
        );
    }

    let tx_coin = coin(amount, pool_info.ibc_denom.clone());

    let msg: NeutronMsg = NeutronMsg::IbcTransfer {
        source_port: "transfer".to_string(),
        source_channel: channel.clone(),
        sender: env.contract.address.to_string(),
        receiver: pool_addr.clone(),
        token: tx_coin,
        timeout_height: RequestPacketTimeoutHeight {
            // todo: revision_number from param?
            revision_number: Some(2),
            revision_height: Some(crate::contract::DEFAULT_TIMEOUT_HEIGHT),
        },
        timeout_timestamp: DEFAULT_TIMEOUT_SECONDS,
        memo: "".to_string(),
        fee: fee.clone(),
    };

    deps.as_ref().api.debug(format!("WASMDEBUG: IbcTransfer msg: {:?}", msg).as_str());

    let interchain_account_id = POOL_ICA_MAP.load(deps.storage, pool_addr.clone())?;

    let submsg_pool_ibc_send = msg_with_sudo_callback(deps.branch(), msg, SudoPayload {
        port_id: get_port_id(env.contract.address.to_string(), interchain_account_id.clone()),
        message: pool_addr.clone(),
        tx_type: TxType::EraUpdateIbcSend,
    })?;
    deps.as_ref().api.debug(
        format!("WASMDEBUG: execute_send: sent submsg: {:?}", submsg_pool_ibc_send).as_str()
    );

    let mut unstaks = Vec::new();
    let mut need_withdraw = Uint128::zero();

    let unstakes_iter = UNSTAKES_OF_INDEX.range(deps.storage, None, None, Order::Ascending);
    for unstake_result in unstakes_iter {
        let (_, unstake_info) = unstake_result?;
        if
            unstake_info.pool_addr == pool_addr &&
            unstake_info.status == WithdrawStatus::Default &&
            unstake_info.era + pool_info.unbonding_period > pool_info.era
        {
            unstaks.push(unstake_info.clone()); // todo: After debugging is complete, can delete this sentence and the following log
            need_withdraw = need_withdraw.add(unstake_info.amount);
        }
    }

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_era_update unstaks is {:?},need_withdraw is:{}",
            unstaks,
            need_withdraw
        ).as_str()
    );

    pool_info.need_withdraw = need_withdraw;
    POOLS.save(deps.storage, pool_addr, &pool_info)?;

    Ok(Response::default().add_submessage(submsg_pool_ibc_send))
}

pub fn sudo_era_update_callback(deps: DepsMut, payload: SudoPayload) -> StdResult<Response> {
    let mut pool_info = POOLS.load(deps.storage, payload.message)?;
    pool_info.era_update_status = EraUpdated;
    POOLS.save(deps.storage, pool_info.pool_addr.clone(), &pool_info)?;
    Ok(Response::new())
}

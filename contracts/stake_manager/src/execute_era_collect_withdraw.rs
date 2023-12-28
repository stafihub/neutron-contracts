use cosmwasm_std::{ DepsMut, Env, Response, Uint128, coin, StdResult };
use neutron_sdk::{
    bindings::{ msg::NeutronMsg, query::NeutronQuery },
    NeutronResult,
    interchain_queries::v045::types::Balances,
    sudo::msg::RequestPacketTimeoutHeight,
    query::min_ibc_fee::query_min_ibc_fee,
    interchain_txs::helpers::get_port_id,
};

use crate::{
    query::query_balance_by_addr,
    contract::{
        DEFAULT_TIMEOUT_HEIGHT,
        DEFAULT_TIMEOUT_SECONDS,
        msg_with_sudo_callback,
        SudoPayload,
        TxType,
    },
    state::POOL_ICA_MAP,
};
use crate::state::PoolBondState::{ BondReported, WithdrawReported };
use crate::state::POOLS;

pub fn execute_era_collect_withdraw(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    channel: String,
    pool_addr: String
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    // check era state
    if pool_info.era_update_status != BondReported {
        deps.as_ref().api.debug(
            format!("WASMDEBUG: execute_era_collect_withdraw skip pool: {:?}", pool_addr).as_str()
        );
        return Ok(Response::default());
    }

    // check withdraw address balance and send it to the pool
    let withdraw_balances: Balances = query_balance_by_addr(
        deps.as_ref(),
        pool_info.withdraw_addr.clone()
    )?.balances;

    let mut withdraw_amount = 0;
    if !withdraw_balances.coins.is_empty() {
        withdraw_amount = u128::from(
            withdraw_balances.coins
                .iter()
                .find(|c| c.denom == pool_info.ibc_denom.clone())
                .map(|c| c.amount)
                .unwrap_or(Uint128::zero())
        );
    }
    if withdraw_amount == 0 {
        pool_info.era_update_status = WithdrawReported;
        POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;
        return Ok(Response::default());
    }

    let fee = crate::contract::min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);

    let tx_withdraw_coin = coin(withdraw_amount, pool_info.ibc_denom.clone());
    let withdraw_token_send = NeutronMsg::IbcTransfer {
        source_port: "transfer".to_string(),
        source_channel: channel.clone(),
        sender: env.contract.address.to_string(),
        receiver: pool_addr.clone(),
        token: tx_withdraw_coin,
        timeout_height: RequestPacketTimeoutHeight {
            // todo: revision_number from param?
            revision_number: Some(2),
            revision_height: Some(DEFAULT_TIMEOUT_HEIGHT),
        },
        timeout_timestamp: DEFAULT_TIMEOUT_SECONDS,
        memo: "".to_string(),
        fee: fee.clone(),
    };

    deps.as_ref().api.debug(
        format!("WASMDEBUG: IbcTransfer msg: {:?}", withdraw_token_send).as_str()
    );

    let interchain_account_id = POOL_ICA_MAP.load(deps.storage, pool_addr.clone())?;

    let submsg_withdraw_ibc_send = msg_with_sudo_callback(
        deps.branch(),
        withdraw_token_send,
        SudoPayload {
            port_id: get_port_id(env.contract.address.to_string(), interchain_account_id),
            message: pool_addr,
            tx_type: TxType::EraUpdateWithdrawSend,
        }
    )?;
    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: submsg_withdraw_ibc_send: sent submsg: {:?}",
            submsg_withdraw_ibc_send
        ).as_str()
    );

    Ok(Response::default().add_submessage(submsg_withdraw_ibc_send))
}

pub fn sudo_era_collect_withdraw_callback(
    deps: DepsMut,
    payload: SudoPayload
) -> StdResult<Response> {
    let mut pool_info = POOLS.load(deps.storage, payload.message)?;
    pool_info.era_update_status = WithdrawReported;
    POOLS.save(deps.storage, pool_info.pool_addr.clone(), &pool_info)?;
    Ok(Response::new())
}

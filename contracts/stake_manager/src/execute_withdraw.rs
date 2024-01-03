use std::vec;

use cosmos_sdk_proto::cosmos::bank::v1beta1::MsgSend;
use cosmos_sdk_proto::cosmos::base::v1beta1::Coin;
use cosmos_sdk_proto::prost::Message;
use cosmwasm_std::{
    Addr, Binary, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128,
};
use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::interchain_txs::helpers::get_port_id;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronError, NeutronResult,
};

use crate::helper::min_ntrn_ibc_fee;
use crate::state::{WithdrawStatus, POOLS, UNSTAKES_INDEX_FOR_USER, UNSTAKES_OF_INDEX};
use crate::{
    contract::{msg_with_sudo_callback, SudoPayload, TxType, DEFAULT_TIMEOUT_SECONDS},
    state::ADDR_ICAID_MAP,
};

pub fn execute_withdraw(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    pool_addr: String,
    receiver: Addr,
    unstake_index_list: Vec<u64>,
) -> NeutronResult<Response<NeutronMsg>> {
    if unstake_index_list.is_empty() {
        return Err(NeutronError::Std(StdError::generic_err(
            "Empty unstake list",
        )));
    }

    let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    let interchain_account_id = ADDR_ICAID_MAP.load(deps.storage, pool_addr.clone())?;

    let mut total_withdraw_amount = Uint128::zero();
    for unstake_index in unstake_index_list.clone() {
        let mut unstake_info =
            UNSTAKES_OF_INDEX.load(deps.storage, (pool_addr.clone(), unstake_index))?;

        if unstake_info.pool_addr != pool_addr {
            return Err(NeutronError::Std(StdError::generic_err(format!(
                "Unstake index: {} pool not match",
                unstake_index
            ))));
        }

        if unstake_info.unstaker != info.sender {
            return Err(NeutronError::Std(StdError::generic_err(format!(
                "Unstake index: {} unstaker not match",
                unstake_index
            ))));
        }

        if unstake_info.status == WithdrawStatus::Pending {
            return Err(NeutronError::Std(StdError::generic_err(format!(
                "Unstake index: {} status not match",
                unstake_index
            ))));
        }
        if unstake_info.era + pool_info.unbonding_period > pool_info.era {
            return Err(NeutronError::Std(StdError::generic_err(format!(
                "Unstake index: {} not withdrawable",
                unstake_index
            ))));
        }

        // Remove the unstake index element of info.sender from UNSTAKES_INDEX_FOR_USER
        total_withdraw_amount += unstake_info.amount;

        unstake_info.status = WithdrawStatus::Pending;
        UNSTAKES_OF_INDEX.save(
            deps.storage,
            (pool_addr.clone(), unstake_index),
            &unstake_info,
        )?;
    }

    if total_withdraw_amount.is_zero() {
        return Err(NeutronError::Std(StdError::generic_err(format!(
            "Encode error: {}",
            "Zero withdraw amount"
        ))));
    }

    let unstake_index_list_str = unstake_index_list
        .iter()
        .map(|index| index.to_string())
        .collect::<Vec<String>>()
        .join("_");

    // interchain tx send atom
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let ica_send = MsgSend {
        from_address: pool_addr.clone(),
        to_address: receiver.to_string(),
        amount: Vec::from([Coin {
            denom: pool_info.remote_denom,
            amount: total_withdraw_amount.to_string(),
        }]),
    };
    let mut buf = Vec::new();
    buf.reserve(ica_send.encoded_len());

    if let Err(e) = ica_send.encode(&mut buf) {
        return Err(NeutronError::Std(StdError::generic_err(format!(
            "Encode error: {}",
            e
        ))));
    }

    let send_msg = ProtobufAny {
        type_url: "/cosmos.bank.v1beta1.MsgSend".to_string(),
        value: Binary::from(buf),
    };

    let cosmos_msg = NeutronMsg::submit_tx(
        pool_info.connection_id.clone(),
        interchain_account_id.clone(),
        vec![send_msg],
        "".to_string(),
        DEFAULT_TIMEOUT_SECONDS,
        fee,
    );

    // We use a submessage here because we need the process message reply to save
    // the outgoing IBC packet identifier for later.
    let submsg = msg_with_sudo_callback(
        deps.branch(),
        cosmos_msg,
        SudoPayload {
            port_id: get_port_id(env.contract.address.as_str(), &interchain_account_id),
            message: format!("{}_{}", info.sender, unstake_index_list_str),
            pool_addr: pool_addr.clone(),
            tx_type: TxType::UserWithdraw,
        },
    )?;

    Ok(Response::new()
        .add_attribute("action", "withdraw")
        .add_attribute("from", info.sender)
        .add_attribute("pool", pool_addr.clone())
        .add_attribute("unstake_index_list", unstake_index_list_str)
        .add_attribute("amount", total_withdraw_amount)
        .add_submessage(submsg))
}

pub fn sudo_withdraw_callback(deps: DepsMut, payload: SudoPayload) -> StdResult<Response> {
    let parts: Vec<String> = payload.message.split('_').map(String::from).collect();
    let user_addr = Addr::unchecked(parts.first().unwrap_or(&String::new()));

    if let Some(mut unstakes) = UNSTAKES_INDEX_FOR_USER
        .may_load(deps.storage, (user_addr.clone(), payload.pool_addr.clone()))?
    {
        deps.api.debug(
            format!(
                "WASMDEBUG: sudo_callback: UserWithdraw before unstakes: {:?}",
                unstakes
            )
            .as_str(),
        );

        unstakes.retain(|unstake_index| {
            if parts.contains(&unstake_index.to_string()) {
                UNSTAKES_OF_INDEX.remove(deps.storage, (payload.pool_addr.clone(), *unstake_index));
                return false;
            }

            true
        });

        deps.api.debug(
            format!(
                "WASMDEBUG: sudo_callback: UserWithdraw after unstakes: {:?}",
                unstakes
            )
            .as_str(),
        );

        UNSTAKES_INDEX_FOR_USER.save(deps.storage, (user_addr, payload.pool_addr), &unstakes)?;
    }
    Ok(Response::new())
}

pub fn sudo_withdraw_failed_callback(deps: DepsMut, payload: SudoPayload) -> StdResult<Response> {
    let parts: Vec<String> = payload.message.split('_').map(String::from).collect();

    if let Some((_, index_list)) = parts.split_first() {
        for index_str in index_list {
            let index = index_str.parse::<u64>().unwrap();
            let mut unstake_info =
                UNSTAKES_OF_INDEX.load(deps.storage, (payload.pool_addr.clone(), index))?;
            unstake_info.status = WithdrawStatus::Default;
            UNSTAKES_OF_INDEX.save(
                deps.storage,
                (payload.pool_addr.clone(), index),
                &unstake_info,
            )?;
        }
    }

    Ok(Response::new())
}

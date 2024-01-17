use std::vec;

use cosmos_sdk_proto::cosmos::bank::v1beta1::MsgSend;
use cosmos_sdk_proto::cosmos::base::v1beta1::Coin;
use cosmos_sdk_proto::prost::Message;
use cosmwasm_std::{Addr, Binary, DepsMut, Env, MessageInfo, Response, Uint128};

use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronResult,
};

use crate::helper::min_ntrn_ibc_fee;
use crate::state::{
    SudoPayload, TxType, WithdrawStatus, INFO_OF_ICA_ID, POOLS, UNSTAKES_INDEX_FOR_USER,
    UNSTAKES_OF_INDEX,
};
use crate::tx_callback::msg_with_sudo_callback;
use crate::{contract::DEFAULT_TIMEOUT_SECONDS, error_conversion::ContractError};

pub fn execute_withdraw(
    mut deps: DepsMut<NeutronQuery>,
    _: Env,
    info: MessageInfo,
    pool_addr: String,
    receiver: Addr,
    unstake_index_list: Vec<u64>,
) -> NeutronResult<Response<NeutronMsg>> {
    if unstake_index_list.is_empty() {
        return Err(ContractError::EmptyUnstakeList {}.into());
    }

    let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    let mut total_withdraw_amount = Uint128::zero();
    for unstake_index in unstake_index_list.clone() {
        let mut unstake_info =
            UNSTAKES_OF_INDEX.load(deps.storage, (pool_addr.clone(), unstake_index))?;

        if unstake_info.pool_addr != pool_addr {
            return Err(ContractError::UnstakeIndexPoolNotMatch(unstake_index).into());
        }

        if unstake_info.unstaker != info.sender {
            return Err(ContractError::UnstakeIndexUnstakerNotMatch(unstake_index).into());
        }

        if unstake_info.status == WithdrawStatus::Pending {
            return Err(ContractError::UnstakeIndexStatusNotMatch(unstake_index).into());
        }
        if unstake_info.era + pool_info.unbonding_period > pool_info.era {
            return Err(ContractError::UnstakeIndexNotWithdrawable(unstake_index).into());
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
        return Err(ContractError::EncodeErrZeroWithdrawAmount {}.into());
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
        return Err(ContractError::EncodeError(e.to_string()).into());
    }

    let send_msg = ProtobufAny {
        type_url: "/cosmos.bank.v1beta1.MsgSend".to_string(),
        value: Binary::from(buf),
    };

    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;
    let cosmos_msg = NeutronMsg::submit_tx(
        pool_ica_info.ctrl_connection_id.clone(),
        pool_info.ica_id.clone(),
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
            port_id: pool_ica_info.ctrl_port_id,
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

pub fn sudo_withdraw_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    let parts: Vec<String> = payload.message.split('_').map(String::from).collect();
    if parts.len() <= 1 {
        return Err(ContractError::UnsupportedMessage(payload.message).into());
    }
    let user_addr = Addr::unchecked(parts.get(0).unwrap());

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

pub fn sudo_withdraw_failed_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
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

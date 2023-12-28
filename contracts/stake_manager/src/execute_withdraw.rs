use std::vec;

use cosmos_sdk_proto::cosmos::bank::v1beta1::MsgSend;
use cosmos_sdk_proto::cosmos::base::v1beta1::Coin;
use cosmos_sdk_proto::prost::Message;
use cosmwasm_std::{
    Addr, Binary, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128,
};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronError,
    NeutronResult, query::min_ibc_fee::query_min_ibc_fee,
};
use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::interchain_txs::helpers::get_port_id;

use crate::contract::{DEFAULT_TIMEOUT_SECONDS, msg_with_sudo_callback, SudoPayload, TxType};
use crate::helper::min_ntrn_ibc_fee;
use crate::state::{POOLS, UNSTAKES_INDEX_FOR_USER, UNSTAKES_OF_INDEX, WithdrawStatus};

pub fn execute_withdraw(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    pool_addr: String,
    receiver: Addr,
    interchain_account_id: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut total_withdraw_amount = Uint128::zero();

    let mut emit_unstake_index_list = vec![];

    let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    if let Some(unstakes) = UNSTAKES_INDEX_FOR_USER.may_load(deps.storage, &info.sender)? {
        for (unstake_pool, unstake_index) in unstakes.into_iter().flatten() {
            if unstake_pool != pool_addr {
                continue;
            }
            let mut unstake_info = UNSTAKES_OF_INDEX.load(deps.storage, unstake_index.clone())?;
            if unstake_info.status == WithdrawStatus::Pending {
                continue;
            }
            if unstake_info.era + pool_info.unbonding_period > pool_info.era {
                continue;
            }

            // Remove the unstake index element of info.sender from UNSTAKES_INDEX_FOR_USER
            total_withdraw_amount += unstake_info.amount;
            emit_unstake_index_list.push(unstake_index.clone());

            unstake_info.status = WithdrawStatus::Pending;
            UNSTAKES_OF_INDEX.save(deps.storage, unstake_index, &unstake_info)?;
        }
    }

    if total_withdraw_amount.is_zero() {
        return Err(NeutronError::Std(StdError::generic_err(format!(
            "Encode error: {}",
            "Zero withdraw amount"
        ))));
    }

    let unstake_index_list_str = emit_unstake_index_list
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

    if let Some(mut unstakes) = UNSTAKES_INDEX_FOR_USER.may_load(deps.storage, &user_addr)? {
        deps.api.debug(
            format!(
                "WASMDEBUG: sudo_callback: UserWithdraw before unstakes: {:?}",
                unstakes
            )
                .as_str(),
        );

        unstakes.retain(|unstake| {
            if let Some((_, unstake_index)) = unstake {
                if parts.contains(unstake_index) {
                    UNSTAKES_OF_INDEX.remove(deps.storage, unstake_index.to_string());
                    return false;
                }
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

        UNSTAKES_INDEX_FOR_USER.save(deps.storage, &user_addr, &unstakes)?;
    }
    Ok(Response::new())
}

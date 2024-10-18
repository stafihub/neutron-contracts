use crate::helper::{check_ibc_fee, gen_msg_send};
use crate::state::{
    SudoPayload, TxType, WithdrawStatus, INFO_OF_ICA_ID, POOLS, UNSTAKES_INDEX_FOR_USER,
    UNSTAKES_OF_INDEX,
};
use crate::tx_callback::msg_with_sudo_callback;
use crate::{error_conversion::ContractError, helper::DEFAULT_TIMEOUT_SECONDS};
use cosmwasm_std::{Addr, DepsMut, MessageInfo, Response, Uint128};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};
use std::vec;

pub fn execute_withdraw(
    mut deps: DepsMut<NeutronQuery>,
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
    let ibc_fee = check_ibc_fee(deps.as_ref(), &info)?;
    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;
    let cosmos_msg = NeutronMsg::submit_tx(
        pool_ica_info.ctrl_connection_id.clone(),
        pool_info.ica_id.clone(),
        vec![gen_msg_send(
            pool_addr.clone(),
            receiver.to_string(),
            pool_info.remote_denom,
            total_withdraw_amount.to_string(),
        )?],
        "".to_string(),
        DEFAULT_TIMEOUT_SECONDS,
        ibc_fee,
    );

    // We use a submessage here because we need the process message reply to save
    // the outgoing IBC packet identifier for later.
    let submsg = msg_with_sudo_callback(
        deps.branch(),
        cosmos_msg,
        SudoPayload {
            port_id: pool_ica_info.ctrl_port_id,
            message: format!(
                "{}_{}_{}_{}",
                total_withdraw_amount, info.sender, receiver, unstake_index_list_str
            ),
            pool_addr: pool_addr.clone(),
            tx_type: TxType::UserWithdraw,
        },
    )?;

    Ok(Response::new().add_submessage(submsg))
}

pub fn sudo_withdraw_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    let parts: Vec<String> = payload.message.split('_').map(String::from).collect();
    if parts.len() <= 3 {
        return Err(ContractError::UnsupportedMessage(payload.message).into());
    }
    let total_withdraw_amount = parts.get(0).unwrap();
    let user_addr = Addr::unchecked(parts.get(1).unwrap());
    let receiver = parts.get(2).unwrap();
    let unstake_index_list_str = parts
        .iter()
        .skip(3)
        .cloned()
        .collect::<Vec<String>>()
        .join("_");

    if let Some((_, index_list)) = parts.split_first() {
        if let Some(mut unstakes) = UNSTAKES_INDEX_FOR_USER
            .may_load(deps.storage, (user_addr.clone(), payload.pool_addr.clone()))?
        {
            unstakes.retain(|unstake_index| {
                if index_list.contains(&unstake_index.to_string()) {
                    UNSTAKES_OF_INDEX
                        .remove(deps.storage, (payload.pool_addr.clone(), *unstake_index));
                    return false;
                }

                true
            });

            UNSTAKES_INDEX_FOR_USER.save(
                deps.storage,
                (user_addr.clone(), payload.pool_addr.clone()),
                &unstakes,
            )?;
        }
    }

    Ok(Response::new()
        .add_attribute("action", "withdraw")
        .add_attribute("from", user_addr)
        .add_attribute("pool", payload.pool_addr.clone())
        .add_attribute("receiver", receiver)
        .add_attribute("unstake_index_list", unstake_index_list_str)
        .add_attribute("amount", total_withdraw_amount))
}

pub fn sudo_withdraw_failed_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    let parts: Vec<String> = payload.message.split('_').map(String::from).collect();

    // skip withdraw_amount and user_addr and reviver
    for index_str in parts.iter().skip(3) {
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

    Ok(Response::new())
}

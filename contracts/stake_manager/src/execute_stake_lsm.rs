use std::{
    ops::{Add, Div, Mul},
    str::FromStr,
};

use cosmwasm_std::{
    coins, to_json_binary, BankMsg, Coin, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
    Uint128, WasmMsg,
};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    sudo::msg::RequestPacketTimeoutHeight,
    NeutronError, NeutronResult,
};

use crate::{
    contract::{msg_with_sudo_callback, SudoPayload, TxType},
    helper::{min_ntrn_ibc_fee, query_denom_trace},
    query::query_validator_by_addr,
    state::{INFO_OF_ICA_ID, POOLS},
};

pub fn execute_stake_lsm(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    neutron_address: String,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;
    if pool_info.paused {
        return Err(NeutronError::Std(StdError::generic_err("Pool is paused")));
    }
    if info.funds.len() != 1 || !info.funds[0].denom.contains("/") {
        return Err(NeutronError::Std(StdError::generic_err(format!(
            "Params error: {}",
            "funds not match"
        ))));
    }
    let share_token_amount = info.funds[0].amount;
    if share_token_amount < pool_info.minimal_stake {
        return Err(NeutronError::Std(StdError::generic_err(
            "less than minimal stake",
        )));
    }

    let denom_parts: Vec<String> = info.funds[0].denom.split("/").map(String::from).collect();
    if denom_parts.len() != 2 {
        return Err(NeutronError::Std(StdError::generic_err("denom not match")));
    }

    let denom_hash = denom_parts.get(1).unwrap();
    let denom_trace = query_denom_trace(deps.as_ref(), denom_hash.to_string())?;

    let share_token_ibc_denom = info.funds[0].denom.to_string();
    let share_token_denom = denom_trace.denom_trace.base_denom;
    let path_parts: Vec<String> = denom_trace
        .denom_trace
        .path
        .split("/")
        .map(String::from)
        .collect();
    if path_parts.len() != 2 {
        return Err(NeutronError::Std(StdError::generic_err(
            "denom path not match",
        )));
    }

    let denom_trace_parts: Vec<String> = share_token_denom.split("/").map(String::from).collect();
    if denom_trace_parts.len() != 2 {
        return Err(NeutronError::Std(StdError::generic_err(
            "denom trace not match",
        )));
    }
    let channel_id_of_share_token = denom_trace_parts.get(1).unwrap();
    let validator_addr = denom_trace_parts.get(0).unwrap();
    if !pool_info.validator_addrs.contains(validator_addr) {
        return Err(NeutronError::Std(StdError::generic_err(
            "validator not support",
        )));
    }
    let validators = query_validator_by_addr(deps.as_ref(), pool_addr.clone())?;
    let sub_msg;
    if let Some(validator) = validators
        .validator
        .validators
        .into_iter()
        .find(|val| val.operator_address == validator_addr.to_string())
    {
        let val_token_amount = Uint128::from_str(&validator.tokens)?;
        let val_share_amount = Uint128::from_str(&validator.delegator_shares)?;

        let token_amount = share_token_amount
            .mul(val_token_amount)
            .div(val_share_amount);

        let fee: neutron_sdk::bindings::msg::IbcFee =
            min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);

        let transfer_share_token_msg = NeutronMsg::IbcTransfer {
            source_port: "transfer".to_string(),
            source_channel: channel_id_of_share_token.to_string(),
            sender: env.contract.address.to_string(),
            receiver: pool_addr.clone(),
            token: info.funds.get(0).unwrap().to_owned(),
            timeout_height: RequestPacketTimeoutHeight {
                // todo: revision_number from param?
                revision_number: Some(2),
                revision_height: Some(crate::contract::DEFAULT_TIMEOUT_HEIGHT),
            },
            timeout_timestamp: 0,
            memo: "".to_string(),
            fee: fee.clone(),
        };

        sub_msg = msg_with_sudo_callback(
            deps.branch(),
            transfer_share_token_msg,
            SudoPayload {
                port_id: pool_ica_info.ctrl_port_id,
                // the acknowledgement later
                message: format!(
                    "{}_{}_{}_{}_{}",
                    neutron_address,
                    token_amount,
                    share_token_amount,
                    share_token_ibc_denom.clone(),
                    share_token_denom.clone(),
                ),
                pool_addr: pool_addr.clone(),
                tx_type: TxType::StakeLsm,
            },
        )?;
    } else {
        return Err(NeutronError::Std(StdError::generic_err(
            "no validator info",
        )));
    }

    Ok(Response::new().add_submessage(sub_msg))
}

pub fn sudo_stake_lsm_callback(deps: DepsMut, payload: SudoPayload) -> StdResult<Response> {
    let parts: Vec<String> = payload.message.split('_').map(String::from).collect();
    if parts.len() != 5 {
        return Err(StdError::generic_err(format!(
            "unsupported  message {}",
            payload.message
        )));
    }

    let staker_neutron_addr = parts.get(0).unwrap();
    let token_amount_str = parts.get(1).unwrap();
    let share_token_amount_str = parts.get(2).unwrap();
    let share_token_denom = parts.get(4).unwrap();

    let token_amount = match token_amount_str.parse::<u128>() {
        Ok(amount) => amount,
        _ => {
            return Err(StdError::generic_err(format!(
                "unsupported  message {}",
                payload.message
            )));
        }
    };
    let share_token_amount = match share_token_amount_str.parse::<u128>() {
        Ok(amount) => amount,
        _ => {
            return Err(StdError::generic_err(format!(
                "unsupported  message {}",
                payload.message
            )));
        }
    };

    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;

    // cal
    let token_amount_use = Uint128::new(token_amount);
    pool_info.active = pool_info.active.add(token_amount_use);
    let rtoken_amount = token_amount_use
        .mul(Uint128::new(1_000_000))
        .div(pool_info.rate);

    // mint
    let msg = WasmMsg::Execute {
        contract_addr: pool_info.rtoken.to_string(),
        msg: to_json_binary(
            &(rtoken::msg::ExecuteMsg::Mint {
                recipient: staker_neutron_addr.to_string(),
                amount: rtoken_amount,
            }),
        )?,
        funds: vec![],
    };

    pool_info.pending_share_tokens.push(Coin {
        denom: share_token_denom.to_string(),
        amount: Uint128::new(share_token_amount),
    });

    // pool_info.pending_share_tokens
    POOLS.save(deps.storage, payload.pool_addr, &pool_info)?;

    Ok(Response::new().add_message(msg))
}

pub fn sudo_stake_lsm_failed_callback(payload: SudoPayload) -> StdResult<Response> {
    let parts: Vec<String> = payload.message.split('_').map(String::from).collect();
    if parts.len() != 5 {
        return Err(StdError::generic_err(format!(
            "unsupported  message {}",
            payload.message
        )));
    }

    let staker_neutron_addr = parts.get(0).unwrap();
    let share_token_amount_str = parts.get(2).unwrap();
    let share_token_ibc_denom = parts.get(3).unwrap();

    let share_token_amount = match share_token_amount_str.parse::<u128>() {
        Ok(amount) => amount,
        _ => {
            return Err(StdError::generic_err(format!(
                "unsupported  message {}",
                payload.message
            )));
        }
    };

    let msg = BankMsg::Send {
        to_address: staker_neutron_addr.to_string(),
        amount: coins(share_token_amount, share_token_ibc_denom),
    };

    Ok(Response::new().add_message(msg))
}

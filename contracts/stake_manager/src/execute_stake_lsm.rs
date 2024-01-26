use crate::{
    error_conversion::ContractError,
    helper::DEFAULT_TIMEOUT_SECONDS,
    helper::{min_ntrn_ibc_fee, query_denom_trace_from_ibc_denom, CAL_BASE},
    query::query_validator_by_addr,
    state::{EraStatus, SudoPayload, TxType, ValidatorUpdateStatus, INFO_OF_ICA_ID, POOLS},
    tx_callback::msg_with_sudo_callback,
};
use cosmwasm_std::{
    coins, to_json_binary, BankMsg, Coin, DepsMut, Env, MessageInfo, Response, Uint128, WasmMsg,
};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    sudo::msg::RequestPacketTimeoutHeight,
    NeutronResult,
};
use std::{
    ops::{Add, Div, Mul},
    str::FromStr,
};

pub fn execute_stake_lsm(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    neutron_address: String,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    if !pool_info.lsm_support {
        return Err(ContractError::LsmStakeNotSupport {}.into());
    }
    if pool_info.share_tokens.len() >= pool_info.lsm_pending_limit as usize {
        return Err(ContractError::LsmPendingStakeOverLimit {}.into());
    }
    if pool_info.status != EraStatus::ActiveEnded {
        return Err(ContractError::EraProcessNotEnd {}.into());
    }
    if pool_info.validator_update_status != ValidatorUpdateStatus::End {
        return Err(ContractError::PoolIcqNotUpdated {}.into());
    }

    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;
    if pool_info.paused {
        return Err(ContractError::PoolIsPaused {}.into());
    }
    if info.funds.len() != 1 || !info.funds[0].denom.contains("/") {
        return Err(ContractError::ParamsErrorFundsNotMatch {}.into());
    }

    let share_token_amount = info.funds[0].amount;
    if share_token_amount < pool_info.minimal_stake {
        return Err(ContractError::LessThanMinimalStake {}.into());
    }

    let share_token_ibc_denom = info.funds[0].denom.to_string();
    let denom_trace =
        query_denom_trace_from_ibc_denom(deps.as_ref(), share_token_ibc_denom.clone())?;

    let share_token_denom = denom_trace.denom_trace.base_denom;
    let path_parts: Vec<String> = denom_trace
        .denom_trace
        .path
        .split("/")
        .map(String::from)
        .collect();
    if path_parts.len() != 2 {
        return Err(ContractError::DenomPathNotMatch {}.into());
    }

    let denom_trace_parts: Vec<String> = share_token_denom.split("/").map(String::from).collect();
    if denom_trace_parts.len() != 2 {
        return Err(ContractError::DenomTraceNotMatch {}.into());
    }
    let channel_id_of_share_token = path_parts.get(1).unwrap();
    let validator_addr = denom_trace_parts.get(0).unwrap();
    if !pool_info.validator_addrs.contains(validator_addr) {
        return Err(ContractError::ValidatorNotSupport {}.into());
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
        let val_share_amount = Uint128::from_str(&validator.delegator_shares)?
            .div(Uint128::from(1_000_000_000_000_000_000u128));

        let token_amount = share_token_amount
            .mul(val_token_amount)
            .div(val_share_amount);
        if token_amount.is_zero() {
            return Err(ContractError::TokenAmountZero {}.into());
        }

        let fee: neutron_sdk::bindings::msg::IbcFee =
            min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);

        let transfer_share_token_msg = NeutronMsg::IbcTransfer {
            source_port: "transfer".to_string(),
            source_channel: channel_id_of_share_token.to_string(),
            sender: env.contract.address.to_string(),
            receiver: pool_addr.clone(),
            token: info.funds.get(0).unwrap().to_owned(),
            timeout_height: RequestPacketTimeoutHeight {
                revision_number: None,
                revision_height: None,
            },
            timeout_timestamp: env.block.time.nanos() + DEFAULT_TIMEOUT_SECONDS * 1_000_000_000,
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
        return Err(ContractError::NoValidatorInfo {}.into());
    }

    Ok(Response::new().add_submessage(sub_msg))
}

pub fn sudo_stake_lsm_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    let parts: Vec<String> = payload.message.split('_').map(String::from).collect();
    if parts.len() != 5 {
        return Err(ContractError::UnsupportedMessage(payload.message).into());
    }

    let staker_neutron_addr = parts.get(0).unwrap();
    let token_amount_str = parts.get(1).unwrap();
    let share_token_amount_str = parts.get(2).unwrap();
    let share_token_denom = parts.get(4).unwrap();

    let token_amount = match token_amount_str.parse::<u128>() {
        Ok(amount) => amount,
        Err(_) => {
            return Err(ContractError::UnsupportedMessage(payload.message).into());
        }
    };
    let share_token_amount = match share_token_amount_str.parse::<u128>() {
        Ok(amount) => amount,
        Err(_) => {
            return Err(ContractError::UnsupportedMessage(payload.message).into());
        }
    };

    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;

    // cal
    let token_amount_use = Uint128::new(token_amount);
    pool_info.active = pool_info.active.add(token_amount_use);
    let lsd_token_amount = token_amount_use.mul(CAL_BASE).div(pool_info.rate);

    // mint
    let msg = WasmMsg::Execute {
        contract_addr: pool_info.lsd_token.to_string(),
        msg: to_json_binary(
            &(lsd_token::msg::ExecuteMsg::Mint {
                recipient: staker_neutron_addr.to_string(),
                amount: lsd_token_amount,
            }),
        )?,
        funds: vec![],
    };
    pool_info.total_lsd_token_amount = pool_info.total_lsd_token_amount.add(lsd_token_amount);

    pool_info.share_tokens.push(Coin {
        denom: share_token_denom.to_string(),
        amount: Uint128::new(share_token_amount),
    });

    // pool_info.share_tokens
    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;

    Ok(Response::new()
        .add_message(msg)
        .add_attribute("action", "stake_lsm")
        .add_attribute("pool", payload.pool_addr)
        .add_attribute("staker", staker_neutron_addr)
        .add_attribute("token_amount", token_amount_use)
        .add_attribute("lsd_token_amount", lsd_token_amount))
}

pub fn sudo_stake_lsm_failed_callback(
    _: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    let parts: Vec<String> = payload.message.split('_').map(String::from).collect();
    if parts.len() != 5 {
        return Err(ContractError::UnsupportedMessage(payload.message).into());
    }

    let staker_neutron_addr = parts.get(0).unwrap();
    let share_token_amount_str = parts.get(2).unwrap();
    let share_token_ibc_denom = parts.get(3).unwrap();

    let share_token_amount = match share_token_amount_str.parse::<u128>() {
        Ok(amount) => amount,
        Err(_) => {
            return Err(ContractError::UnsupportedMessage(payload.message).into());
        }
    };

    let msg = BankMsg::Send {
        to_address: staker_neutron_addr.to_string(),
        amount: coins(share_token_amount, share_token_ibc_denom),
    };

    Ok(Response::new().add_message(msg))
}

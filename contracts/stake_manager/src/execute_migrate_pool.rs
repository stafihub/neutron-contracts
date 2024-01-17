use std::ops::{Div, Mul};
use std::vec;

use cosmos_sdk_proto::cosmos::distribution::v1beta1::MsgSetWithdrawAddress;
use cosmos_sdk_proto::prost::Message;
use cosmwasm_std::{instantiate2_address, to_json_binary, Addr, Uint128, WasmMsg};
use cosmwasm_std::{Binary, DepsMut, Env, MessageInfo, Response};

use lsd_token::msg::InstantiateMinterData;
use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::interchain_queries::v045::new_register_delegator_delegations_query_msg;
use neutron_sdk::interchain_queries::v045::{
    new_register_balance_query_msg, new_register_staking_validators_query_msg,
};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronError, NeutronResult,
};

use crate::contract::{DEFAULT_TIMEOUT_SECONDS, DEFAULT_UPDATE_PERIOD};
use crate::error_conversion::ContractError;
use crate::helper::{CAL_BASE, DEFAULT_DECIMALS};
use crate::msg::MigratePoolParams;
use crate::query_callback::register_query_submsg;
use crate::state::{QueryKind, SudoPayload, TxType, POOLS};
use crate::state::{INFO_OF_ICA_ID, STACK};
use crate::tx_callback::msg_with_sudo_callback;
use crate::{helper::min_ntrn_ibc_fee, state::ValidatorUpdateStatus};

// add execute to config the validator addrs and withdraw address on reply
pub fn execute_migrate_pool(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    param: MigratePoolParams,
) -> NeutronResult<Response<NeutronMsg>> {
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);

    let (pool_ica_info, withdraw_ica_info, _) =
        INFO_OF_ICA_ID.load(deps.storage, param.interchain_account_id.clone())?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_init_pool pool_ica_info: {:?}",
            pool_ica_info
        )
        .as_str(),
    );

    if param.validator_addrs.is_empty() || param.validator_addrs.len() > 5 {
        return Err(ContractError::ValidatorAddressesListSize {}.into());
    }

    let mut pool_info = POOLS.load(deps.as_ref().storage, pool_ica_info.ica_addr.clone())?;
    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }
    if !pool_info.rate.is_zero() {
        return Err(ContractError::PoolInited {}.into());
    }
    if param.rate.is_zero() {
        return Err(ContractError::RateIsZero {}.into());
    }

    let code_id = match param.lsd_code_id {
        Some(lsd_code_id) => lsd_code_id,
        None => STACK.load(deps.storage)?.lsd_token_code_id,
    };
    let salt = &pool_ica_info.ica_addr.clone()[..40];

    let code_info = deps.querier.query_wasm_code_info(code_id)?;
    let creator_cannonical = deps.api.addr_canonicalize(env.contract.address.as_str())?;

    let i2_address =
        instantiate2_address(&code_info.checksum, &creator_cannonical, salt.as_bytes())
            .map_err(|e| ContractError::Instantiate2AddressFailed(e.to_string()))?;

    let contract_addr = deps
        .api
        .addr_humanize(&i2_address)
        .map_err(NeutronError::Std)?;

    let instantiate_lsd_msg = WasmMsg::Instantiate2 {
        admin: Option::from(info.sender.to_string()),
        code_id,
        msg: to_json_binary(
            &(lsd_token::msg::InstantiateMsg {
                name: param.lsd_token_name.clone(),
                symbol: param.lsd_token_symbol,
                decimals: DEFAULT_DECIMALS,
                initial_balances: vec![],
                mint: Option::from(InstantiateMinterData {
                    admin: pool_info.admin.to_string(),
                    minter: env.contract.address.to_string(),
                    cap: None,
                }),
                marketing: None,
            }),
        )?,
        funds: vec![],
        label: param.lsd_token_name.clone(),
        salt: salt.as_bytes().into(),
    };

    pool_info.bond = param.bond;
    pool_info.unbond = param.unbond;
    pool_info.active = param.active;
    pool_info.era = param.era;
    pool_info.rate = param.rate;
    pool_info.ibc_denom = param.ibc_denom;
    pool_info.channel_id_of_ibc_denom = param.channel_id_of_ibc_denom;
    pool_info.remote_denom = param.remote_denom;
    pool_info.validator_addrs = param.validator_addrs.clone();
    pool_info.platform_fee_receiver = Addr::unchecked(param.platform_fee_receiver);
    pool_info.lsd_token = contract_addr;
    pool_info.share_tokens = param.share_tokens;
    pool_info.total_platform_fee = param.total_platform_fee;
    pool_info.total_lsd_token_amount = param.total_lsd_token_amount;
    pool_info.era_seconds = param.era_seconds;
    pool_info.offset = param.offset;
    pool_info.unbonding_period = param.unbonding_period;
    pool_info.minimal_stake = param.minimal_stake;

    // option
    if let Some(platform_fee_commission) = param.platform_fee_commission {
        pool_info.platform_fee_commission = platform_fee_commission;
    } else {
        pool_info.platform_fee_commission = Uint128::new(100_000);
    }

    // default
    pool_info.next_unstake_index = 0;
    pool_info.unstake_times_limit = 20;
    pool_info.unbond_commission = Uint128::zero();
    pool_info.paused = false;
    pool_info.lsm_support = true;
    pool_info.lsm_pending_limit = 50;
    pool_info.rate_change_limit = Uint128::new(5000);
    pool_info.validator_update_status = ValidatorUpdateStatus::End;

    // check rate
    let cal_rate = if pool_info.total_lsd_token_amount.is_zero() {
        CAL_BASE
    } else {
        pool_info
            .active
            .mul(CAL_BASE)
            .div(pool_info.total_lsd_token_amount)
    };
    if cal_rate != pool_info.rate {
        return Err(ContractError::RateNotMatch {}.into());
    }

    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: execute_init_pool POOLS.load: {:?}", pool_info).as_str());

    POOLS.save(deps.storage, pool_ica_info.ica_addr.clone(), &pool_info)?;

    let register_balance_pool_submsg = register_query_submsg(
        deps.branch(),
        new_register_balance_query_msg(
            pool_ica_info.ctrl_connection_id.clone(),
            pool_ica_info.ica_addr.clone(),
            pool_info.remote_denom.clone(),
            DEFAULT_UPDATE_PERIOD,
        )?,
        pool_ica_info.ica_addr.clone(),
        QueryKind::Balances,
    )?;
    let register_balance_withdraw_submsg = register_query_submsg(
        deps.branch(),
        new_register_balance_query_msg(
            withdraw_ica_info.ctrl_connection_id.clone(),
            withdraw_ica_info.ica_addr.clone(),
            pool_info.remote_denom.clone(),
            DEFAULT_UPDATE_PERIOD,
        )?,
        withdraw_ica_info.ica_addr.clone(),
        QueryKind::Balances,
    )?;
    let register_delegation_submsg = register_query_submsg(
        deps.branch(),
        new_register_delegator_delegations_query_msg(
            pool_ica_info.ctrl_connection_id.clone(),
            pool_ica_info.ica_addr.clone(),
            pool_info.validator_addrs.clone(),
            DEFAULT_UPDATE_PERIOD,
        )?,
        pool_ica_info.ica_addr.clone(),
        QueryKind::Delegations,
    )?;

    let register_validator_submsg = register_query_submsg(
        deps.branch(),
        new_register_staking_validators_query_msg(
            pool_ica_info.ctrl_connection_id.clone(),
            pool_info.validator_addrs.clone(),
            6,
        )?,
        pool_ica_info.ica_addr.clone(),
        QueryKind::Validators,
    )?;

    let set_withdraw_msg = MsgSetWithdrawAddress {
        delegator_address: pool_ica_info.ica_addr.clone(),
        withdraw_address: withdraw_ica_info.ica_addr.clone(),
    };
    let mut buf = Vec::new();
    buf.reserve(set_withdraw_msg.encoded_len());

    if let Err(e) = set_withdraw_msg.encode(&mut buf) {
        return Err(ContractError::EncodeError(e.to_string()).into());
    }

    let cosmos_msg = NeutronMsg::submit_tx(
        pool_ica_info.ctrl_connection_id.clone(),
        param.interchain_account_id.clone(),
        vec![ProtobufAny {
            type_url: "/cosmos.distribution.v1beta1.MsgSetWithdrawAddress".to_string(),
            value: Binary::from(buf),
        }],
        "".to_string(),
        DEFAULT_TIMEOUT_SECONDS,
        fee.clone(),
    );

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_init_pool cosmos_msg is {:?}",
            cosmos_msg
        )
        .as_str(),
    );

    // We use a submessage here because we need the process message reply to save
    // the outgoing IBC packet identifier for later.
    let submsg_set_withdraw = msg_with_sudo_callback(
        deps.branch(),
        cosmos_msg,
        SudoPayload {
            port_id: pool_ica_info.ctrl_port_id,
            message: withdraw_ica_info.ica_addr,
            pool_addr: pool_ica_info.ica_addr.clone(),
            tx_type: TxType::SetWithdrawAddr,
        },
    )?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_init_pool submsg_set_withdraw: {:?}",
            submsg_set_withdraw
        )
        .as_str(),
    );

    Ok(Response::default()
        .add_message(instantiate_lsd_msg)
        .add_submessages(vec![
            register_balance_pool_submsg,
            register_balance_withdraw_submsg,
            register_delegation_submsg,
            register_validator_submsg,
            submsg_set_withdraw,
        ]))
}

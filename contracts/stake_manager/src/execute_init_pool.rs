use std::vec;

use cosmos_sdk_proto::cosmos::distribution::v1beta1::MsgSetWithdrawAddress;
use cosmos_sdk_proto::prost::Message;
use cosmwasm_std::{Addr, Uint128};
use cosmwasm_std::{Binary, DepsMut, Env, MessageInfo, Response, StdError, SubMsg};

use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::interchain_queries::v045::new_register_balance_query_msg;
use neutron_sdk::interchain_queries::v045::new_register_delegator_delegations_query_msg;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronError, NeutronResult,
};

use crate::error_conversion::ContractError;
use crate::msg::InitPoolParams;
use crate::state::POOLS;
use crate::state::{
    ADDR_BALANCES_REPLY_ID, INFO_OF_ICA_ID, LATEST_BALANCES_REPLY_ID, LATEST_DELEGATIONS_REPLY_ID,
};
use crate::{
    contract::{
        msg_with_sudo_callback, SudoPayload, TxType, DEFAULT_TIMEOUT_SECONDS, DEFAULT_UPDATE_PERIOD,
    },
    state::{PoolValidatorStatus, ADDR_DELEGATIONS_REPLY_ID, POOL_VALIDATOR_STATUS},
};
use crate::{helper::min_ntrn_ibc_fee, state::ValidatorUpdateStatus};

// add execute to config the validator addrs and withdraw address on reply
pub fn execute_init_pool(
    mut deps: DepsMut<NeutronQuery>,
    _: Env,
    info: MessageInfo,
    param: InitPoolParams,
) -> NeutronResult<Response<NeutronMsg>> {
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);

    let (pool_ica_info, withdraw_ica_info, _) =
        INFO_OF_ICA_ID.load(deps.storage, param.interchain_account_id.clone())?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_init_pool get_ica delegator: {:?}, connection_id: {:?}",
            pool_ica_info.ica_addr, pool_ica_info.ctrl_connection_id
        )
        .as_str(),
    );

    if param.validator_addrs.is_empty() || param.validator_addrs.len() > 5 {
        return Err(NeutronError::Std(StdError::generic_err(
            "Validator addresses list must contain between 1 and 5 addresses.",
        )));
    }

    let mut pool_info = POOLS.load(deps.as_ref().storage, pool_ica_info.ica_addr.clone())?;
    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }

    pool_info.bond = param.bond;
    pool_info.unbond = param.unbond;
    pool_info.active = param.active;
    pool_info.era = param.era;
    pool_info.rate = param.rate;
    pool_info.ibc_denom = param.ibc_denom;
    pool_info.channel_id_of_ibc_denom = param.channel_id_of_ibc_denom;
    pool_info.remote_denom = param.remote_denom;
    pool_info.validator_addrs = param.validator_addrs.clone();
    pool_info.protocol_fee_receiver = Addr::unchecked(param.protocol_fee_receiver);
    pool_info.rtoken = Addr::unchecked(param.rtoken);

    // default
    pool_info.minimal_stake = Uint128::new(10_000);
    pool_info.next_unstake_index = 0;
    pool_info.unbonding_period = 15;
    pool_info.unstake_times_limit = 20;
    pool_info.unbond_commission = Uint128::zero();
    pool_info.era_seconds = 24 * 60 * 60;
    pool_info.offset = 0;
    pool_info.paused = true;

    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: execute_init_pool POOLS.load: {:?}", pool_info).as_str());

    POOLS.save(deps.storage, pool_ica_info.ica_addr.clone(), &pool_info)?;

    let latest_balance_query_id = LATEST_BALANCES_REPLY_ID.load(deps.as_ref().storage)?;
    let latest_delegation_query_id = LATEST_DELEGATIONS_REPLY_ID.load(deps.as_ref().storage)?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_init_pool pool update: {:?},latest_query_id is {:?}",
            pool_info, latest_balance_query_id
        )
        .as_str(),
    );

    let pool_delegation_query_id = latest_delegation_query_id + 1;
    let pool_query_id = latest_balance_query_id + 1;
    let withdraw_query_id = latest_balance_query_id + 2;

    LATEST_BALANCES_REPLY_ID.save(deps.storage, &(withdraw_query_id + 1))?;
    LATEST_DELEGATIONS_REPLY_ID.save(deps.storage, &(pool_delegation_query_id + 1))?;

    let register_delegation_query_msg = new_register_delegator_delegations_query_msg(
        pool_ica_info.ctrl_connection_id.clone(),
        pool_ica_info.ica_addr.clone(),
        param.validator_addrs,
        DEFAULT_UPDATE_PERIOD,
    )?;

    // wrap into submessage to save {query_id, query_type} on reply that'll later be used to handle sudo kv callback
    let register_delegation_query_submsg =
        SubMsg::reply_on_success(register_delegation_query_msg, pool_delegation_query_id);

    ADDR_DELEGATIONS_REPLY_ID.save(
        deps.storage,
        pool_ica_info.ica_addr.clone(),
        &pool_delegation_query_id,
    )?;

    let register_balance_pool_msg = new_register_balance_query_msg(
        pool_ica_info.ctrl_connection_id.clone(),
        pool_ica_info.ica_addr.clone(),
        pool_info.remote_denom.clone(),
        DEFAULT_UPDATE_PERIOD,
    )?;

    // wrap into submessage to save {query_id, query_type} on reply that'll later be used to handle sudo kv callback
    let register_balance_pool_submsg =
        SubMsg::reply_on_success(register_balance_pool_msg, pool_query_id);

    ADDR_BALANCES_REPLY_ID.save(deps.storage, pool_ica_info.ica_addr.clone(), &pool_query_id)?;

    let register_balance_withdraw_msg = new_register_balance_query_msg(
        withdraw_ica_info.ctrl_connection_id.clone(),
        withdraw_ica_info.ica_addr.clone(),
        pool_info.remote_denom.clone(),
        DEFAULT_UPDATE_PERIOD,
    )?;

    // wrap into submessage to save {query_id, query_type} on reply that'll later be used to handle sudo kv callback
    let register_balance_withdraw_submsg =
        SubMsg::reply_on_success(register_balance_withdraw_msg, withdraw_query_id);

    ADDR_BALANCES_REPLY_ID.save(
        deps.storage,
        withdraw_ica_info.ica_addr.clone(),
        &withdraw_query_id,
    )?;

    let set_withdraw_msg = MsgSetWithdrawAddress {
        delegator_address: pool_ica_info.ica_addr.clone(),
        withdraw_address: withdraw_ica_info.ica_addr.clone(),
    };
    let mut buf = Vec::new();
    buf.reserve(set_withdraw_msg.encoded_len());

    if let Err(e) = set_withdraw_msg.encode(&mut buf) {
        return Err(NeutronError::Std(StdError::generic_err(format!(
            "Encode error: {}",
            e
        ))));
    }

    let any_msg = ProtobufAny {
        type_url: "/cosmos.distribution.v1beta1.MsgSetWithdrawAddress".to_string(),
        value: Binary::from(buf),
    };

    let cosmos_msg = NeutronMsg::submit_tx(
        pool_ica_info.ctrl_channel_id.clone(),
        param.interchain_account_id.clone(),
        vec![any_msg],
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

    POOL_VALIDATOR_STATUS.save(
        deps.storage,
        pool_ica_info.ica_addr.clone(),
        &PoolValidatorStatus {
            status: ValidatorUpdateStatus::WaitQueryUpdate,
        },
    )?;

    Ok(Response::default().add_submessages(vec![
        register_delegation_query_submsg,
        register_balance_pool_submsg,
        register_balance_withdraw_submsg,
        submsg_set_withdraw,
    ]))
}

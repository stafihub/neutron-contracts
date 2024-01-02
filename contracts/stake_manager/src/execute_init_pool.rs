use std::vec;

use cosmos_sdk_proto::cosmos::distribution::v1beta1::MsgSetWithdrawAddress;
use cosmos_sdk_proto::prost::Message;
use cosmwasm_std::{Binary, DepsMut, Env, Response, StdError, SubMsg};
use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::interchain_queries::v045::new_register_balance_query_msg;
use neutron_sdk::interchain_queries::v045::new_register_delegator_delegations_query_msg;
use neutron_sdk::interchain_txs::helpers::get_port_id;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronError, NeutronResult,
};

use crate::msg::InitPoolParams;
use crate::state::POOLS;
use crate::state::{ADDR_BALANCES_QUERY_ID, LATEST_BALANCES_QUERY_ID, LATEST_DELEGATIONS_QUERY_ID};
use crate::{
    contract::{
        msg_with_sudo_callback, SudoPayload, TxType, DEFAULT_TIMEOUT_SECONDS, DEFAULT_UPDATE_PERIOD,
    },
    state::{PoolValidatorStatus, ADDR_DELEGATIONS_QUERY_ID, POOL_VALIDATOR_STATUS},
};
use crate::{
    helper::{get_ica, min_ntrn_ibc_fee},
    state::ValidatorUpdateStatus,
};

// add execute to config the validator addrs and withdraw address on reply
pub fn execute_init_pool(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    param: InitPoolParams,
) -> NeutronResult<Response<NeutronMsg>> {
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let (delegator, connection_id) = get_ica(deps.as_ref(), &env, &param.interchain_account_id)?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_init_pool get_ica delegator: {:?},connection_id: {:?}",
            delegator, connection_id
        )
        .as_str(),
    );

    if param.validator_addrs.is_empty() || param.validator_addrs.len() > 5 {
        return Err(NeutronError::Std(StdError::generic_err(
            "Validator addresses list must contain between 1 and 5 addresses.",
        )));
    }

    let mut pool_info = POOLS.load(deps.as_ref().storage, delegator.clone())?;

    pool_info.unbond = param.unbond;
    pool_info.active = param.active;
    pool_info.era = param.era;
    pool_info.rate = param.rate;
    pool_info.ibc_denom = param.ibc_denom;
    pool_info.remote_denom = param.remote_denom;
    pool_info.connection_id = connection_id.clone();
    pool_info.validator_addrs = param.validator_addrs.clone();

    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: execute_init_pool POOLS.load: {:?}", pool_info).as_str());

    POOLS.save(deps.storage, pool_info.pool_addr.clone(), &pool_info)?;

    let latest_balance_query_id = LATEST_BALANCES_QUERY_ID.load(deps.as_ref().storage)?;
    let latest_delegation_query_id = LATEST_DELEGATIONS_QUERY_ID.load(deps.as_ref().storage)?;

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

    LATEST_BALANCES_QUERY_ID.save(deps.storage, &(withdraw_query_id + 1))?;
    LATEST_DELEGATIONS_QUERY_ID.save(deps.storage, &(pool_delegation_query_id + 1))?;

    let register_delegation_query_msg = new_register_delegator_delegations_query_msg(
        connection_id.clone(),
        delegator.clone(),
        param.validator_addrs,
        DEFAULT_UPDATE_PERIOD,
    )?;

    // wrap into submessage to save {query_id, query_type} on reply that'll later be used to handle sudo kv callback
    let register_delegation_query_submsg =
        SubMsg::reply_on_success(register_delegation_query_msg, pool_delegation_query_id);

    ADDR_DELEGATIONS_QUERY_ID.save(deps.storage, delegator.clone(), &pool_delegation_query_id)?;

    let register_balance_pool_msg = new_register_balance_query_msg(
        connection_id.clone(),
        delegator.clone(),
        pool_info.remote_denom.clone(),
        DEFAULT_UPDATE_PERIOD,
    )?;

    // wrap into submessage to save {query_id, query_type} on reply that'll later be used to handle sudo kv callback
    let register_balance_pool_submsg =
        SubMsg::reply_on_success(register_balance_pool_msg, pool_query_id);

    ADDR_BALANCES_QUERY_ID.save(deps.storage, delegator.clone(), &pool_query_id)?;

    let register_balance_withdraw_msg = new_register_balance_query_msg(
        connection_id.clone(),
        pool_info.withdraw_addr.clone(),
        pool_info.remote_denom.clone(),
        DEFAULT_UPDATE_PERIOD,
    )?;

    // wrap into submessage to save {query_id, query_type} on reply that'll later be used to handle sudo kv callback
    let register_balance_withdraw_submsg =
        SubMsg::reply_on_success(register_balance_withdraw_msg, withdraw_query_id);

    ADDR_BALANCES_QUERY_ID.save(
        deps.storage,
        pool_info.withdraw_addr.clone(),
        &withdraw_query_id,
    )?;

    let set_withdraw_msg = MsgSetWithdrawAddress {
        delegator_address: delegator.clone(),
        withdraw_address: pool_info.withdraw_addr.clone(),
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
        connection_id.clone(),
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
            port_id: get_port_id(
                env.contract.address.to_string(),
                param.interchain_account_id,
            ),
            message: pool_info.withdraw_addr,
            pool_addr: pool_info.pool_addr.clone(),
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
        pool_info.pool_addr,
        &PoolValidatorStatus {
            status: ValidatorUpdateStatus::Success,
        },
    )?;

    Ok(Response::default().add_submessages(vec![
        register_delegation_query_submsg,
        register_balance_pool_submsg,
        register_balance_withdraw_submsg,
        submsg_set_withdraw,
    ]))
}

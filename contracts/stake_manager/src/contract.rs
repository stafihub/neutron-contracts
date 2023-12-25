use std::ops::{ Add, Div, Mul, Sub };
use cosmwasm_std::{
    coin,
    to_json_binary,
    entry_point,
    from_json,
    Binary,
    CosmosMsg,
    Deps,
    DepsMut,
    Env,
    MessageInfo,
    Reply,
    Response,
    StdError,
    StdResult,
    SubMsg,
    Uint128,
    WasmMsg,
    CustomQuery,
    Addr,
    QueryRequest,
    WasmQuery,
};
use cw2::set_contract_version;
use neutron_sdk::{
    bindings::{
        msg::{ IbcFee, MsgIbcTransferResponse, NeutronMsg },
        query::{ NeutronQuery, QueryInterchainAccountAddressResponse },
    },
    query::min_ibc_fee::query_min_ibc_fee,
    sudo::msg::{ RequestPacket, RequestPacketTimeoutHeight },
    NeutronResult,
    NeutronError,
    interchain_queries::{
        get_registered_query,
        v045::{ queries::BalanceResponse, types::Balances, types::Delegations },
        check_query_type,
        types::QueryType,
        query_kv_result,
    },
};
use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::interchain_txs::helpers::get_port_id;
use schemars::JsonSchema;
use serde::{ Deserialize, Serialize };
use cosmos_sdk_proto::cosmos::{ bank::v1beta1::{ MsgSend }, staking::v1beta1::MsgBeginRedelegate };
use neutron_sdk::interchain_queries::{ v045::{ new_register_delegator_delegations_query_msg } };
use cosmos_sdk_proto::cosmos::base::v1beta1::Coin;
use cosmos_sdk_proto::cosmos::distribution::v1beta1::MsgSetWithdrawAddress;
use cosmos_sdk_proto::cosmos::staking::v1beta1::{ MsgDelegate, MsgUndelegate };
use cosmos_sdk_proto::prost::Message;
use neutron_sdk::interchain_queries::v045::new_register_balance_query_msg;
use neutron_sdk::sudo::msg::SudoMsg;
use crate::{
    msg::{ ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg },
    state::{
        read_reply_payload,
        read_sudo_payload,
        save_reply_payload,
        save_sudo_payload,
        IBC_SUDO_ID_RANGE_END,
        IBC_SUDO_ID_RANGE_START,
        KV_QUERY_ID_TO_CALLBACKS,
        QueryKind,
        LATEST_QUERY_ID,
        ADDR_QUERY_ID,
        PoolBondState,
        ACKNOWLEDGEMENT_RESULTS,
        read_errors_from_queue,
    },
};
use crate::state::{
    INTERCHAIN_ACCOUNTS,
    STATE,
    State,
    UnstakeInfo,
    UNSTAKES_INDEX_FOR_USER,
    UNSTAKES_OF_INDEX,
    POOLS,
    POOL_DENOM_MPA,
    POOL_ICA_MAP,
};
use crate::state::PoolBondState::{ BondReported, EraUpdated };

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
struct OpenAckVersion {
    version: String,
    controller_connection_id: String,
    host_connection_id: String,
    address: String,
    encoding: String,
    tx_type: String,
}

#[derive(Clone, Debug)]
pub struct ValidatorUnbondInfo {
    pub validator: String,
    pub delegation_amount: Uint128,
    pub unbond_amount: Uint128,
}

// Default timeout for IbcTransfer is 10000000 blocks
const DEFAULT_TIMEOUT_HEIGHT: u64 = 10000000;

pub const SUDO_PAYLOAD_REPLY_ID: u64 = 1;

// Default timeout for SubmitTX is two weeks
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 60 * 60 * 24 * 7 * 2;

pub const DEFAULT_UPDATE_PERIOD: u64 = 6;

// config by instantiate
// const UATOM_IBC_DENOM: &str =
// 	"ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2";

const FEE_DENOM: &str = "untrn";

const CONTRACT_NAME: &str = concat!("crates.io:neutron-sdk__", env!("CARGO_PKG_NAME"));
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct IbcSendType {
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InterTxType {
    pub message: String,
    pub port_id: String,
}

// Enum representing payload to process during handling acknowledgement messages in Sudo handler
#[derive(Serialize, Deserialize)]
pub enum SudoPayload {
    HandlerPayloadIbcSend(IbcSendType),
    HandlerPayloadInterTx(InterTxType),
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _: Env,
    info: MessageInfo,
    _: InstantiateMsg
) -> NeutronResult<Response<NeutronMsg>> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    STATE.save(
        deps.storage,
        &(State {
            owner: info.sender,
        })
    )?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    deps.api.debug("WASMDEBUG: migrate");
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<NeutronQuery>, env: Env, msg: QueryMsg) -> NeutronResult<Binary> {
    match msg {
        QueryMsg::GetRegisteredQuery { query_id } => {
            Ok(to_json_binary(&get_registered_query(deps, query_id)?)?)
        }
        QueryMsg::Balance { query_id } => Ok(to_json_binary(&query_balance(deps, env, query_id)?)?),
        QueryMsg::InterchainAccountAddress { interchain_account_id, connection_id } =>
            query_interchain_address(deps, env, interchain_account_id, connection_id),
        QueryMsg::InterchainAccountAddressFromContract { interchain_account_id } =>
            query_interchain_address_contract(deps, env, interchain_account_id),
        QueryMsg::AcknowledgementResult { interchain_account_id, sequence_id } =>
            query_acknowledgement_result(deps, env, interchain_account_id, sequence_id),
        QueryMsg::ErrorsQueue {} => query_errors_queue(deps),
    }
}

pub fn query_balance(
    deps: Deps<NeutronQuery>,
    _env: Env,
    registered_query_id: u64
) -> NeutronResult<BalanceResponse> {
    // get info about the query
    let registered_query = get_registered_query(deps, registered_query_id)?;
    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Balances structure from raw KV-storage values
    let balances: Balances = query_kv_result(deps, registered_query_id)?;

    Ok(BalanceResponse {
        // last_submitted_height tells us when the query result was updated last time (block height)
        last_submitted_local_height: registered_query.registered_query.last_submitted_result_local_height,
        balances,
    })
}

// returns ICA address from Neutron ICA SDK module
pub fn query_interchain_address(
    deps: Deps<NeutronQuery>,
    env: Env,
    interchain_account_id: String,
    connection_id: String
) -> NeutronResult<Binary> {
    let query = NeutronQuery::InterchainAccountAddress {
        owner_address: env.contract.address.to_string(),
        interchain_account_id,
        connection_id,
    };

    let res: QueryInterchainAccountAddressResponse = deps.querier.query(&query.into())?;
    Ok(to_json_binary(&res)?)
}

// returns ICA address from the contract storage. The address was saved in sudo_open_ack method
pub fn query_interchain_address_contract(
    deps: Deps<NeutronQuery>,
    env: Env,
    interchain_account_id: String
) -> NeutronResult<Binary> {
    Ok(to_json_binary(&get_ica(deps, &env, &interchain_account_id)?)?)
}

// returns the result
pub fn query_acknowledgement_result(
    deps: Deps<NeutronQuery>,
    env: Env,
    interchain_account_id: String,
    sequence_id: u64
) -> NeutronResult<Binary> {
    let port_id = get_port_id(env.contract.address.as_str(), &interchain_account_id);
    let res = ACKNOWLEDGEMENT_RESULTS.may_load(deps.storage, (port_id, sequence_id))?;
    Ok(to_json_binary(&res)?)
}

pub fn query_errors_queue(deps: Deps<NeutronQuery>) -> NeutronResult<Binary> {
    let res = read_errors_from_queue(deps.storage)?;
    Ok(to_json_binary(&res)?)
}

// todo: add response event
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg
) -> NeutronResult<Response<NeutronMsg>> {
    match msg {
        // NOTE: this is an example contract that shows how to make IBC transfers!
        // todo: Please add necessary authorization or other protection mechanisms
        // if you intend to send funds over IBC
        ExecuteMsg::RegisterPool { connection_id, interchain_account_id } =>
            execute_register_pool(deps, env, info, connection_id, interchain_account_id),
        ExecuteMsg::ConfigPool {
            interchain_account_id,
            validator_addrs,
            withdraw_addr,
            rtoken,
            minimal_stake,
            unstake_times_limit,
            next_unstake_index,
            unbonding_period,
        } =>
            execute_config_pool(
                deps,
                env,
                info,
                interchain_account_id,
                validator_addrs,
                withdraw_addr,
                rtoken,
                minimal_stake,
                unstake_times_limit,
                next_unstake_index,
                unbonding_period
            ),
        ExecuteMsg::RegisterBalanceQuery { connection_id, addr, denom, update_period } =>
            register_balance_query(connection_id, addr, denom, update_period),
        ExecuteMsg::RegisterDelegatorDelegationsQuery {
            connection_id,
            delegator,
            validators,
            update_period,
        } => register_delegations_query(connection_id, delegator, validators, update_period),
        ExecuteMsg::Stake { neutron_address, pool_addr } =>
            execute_stake(deps, env, neutron_address, pool_addr, info),
        ExecuteMsg::Unstake { amount, interchain_account_id, pool_addr } =>
            execute_unstake(deps, env, info, amount, interchain_account_id, pool_addr),
        ExecuteMsg::Withdraw { pool_addr, receiver, interchain_account_id } =>
            execute_withdraw(deps, env, info, pool_addr, receiver, interchain_account_id),
        ExecuteMsg::PoolRmValidator { pool_addr, validator_addrs } =>
            execute_rm_pool_validators(deps, env, info, pool_addr, validator_addrs),
        ExecuteMsg::EraUpdate { channel, pool_addr } =>
            // Different rtoken are executed separately.
            execute_era_update(deps, env, channel, pool_addr),
        ExecuteMsg::EraBond { pool_addr } => execute_era_bond(deps, env, pool_addr),
        ExecuteMsg::EraBondActive { pool_addr } => execute_bond_active(deps, env, pool_addr),
        ExecuteMsg::StakeLSM {} => execute_stake_lsm(deps, env, info),
    }
}

fn execute_register_pool(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    _: MessageInfo,
    connection_id: String,
    interchain_account_id: String
) -> NeutronResult<Response<NeutronMsg>> {
    let register = NeutronMsg::register_interchain_account(
        connection_id.clone(),
        interchain_account_id.clone()
    );
    let key = get_port_id(env.contract.address.as_str(), &interchain_account_id);
    // we are saving empty data here because we handle response of registering ICA in sudo_open_ack method
    INTERCHAIN_ACCOUNTS.save(deps.storage, key, &None)?;

    Ok(Response::default().add_message(register))
}

// add execute to config the validator addrs and withdraw address on reply
fn execute_config_pool(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    _: MessageInfo,
    interchain_account_id: String,
    validator_addrs: Vec<String>,
    withdraw_addr: String,
    rtoken: Addr,
    minimal_stake: Uint128,
    unstake_times_limit: Uint128,
    next_unstake_index: Uint128,
    unbonding_period: u128
) -> NeutronResult<Response<NeutronMsg>> {
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let (delegator, connection_id) = get_ica(deps.as_ref(), &env, &interchain_account_id)?;
    let mut pool_info = POOLS.load(deps.storage, delegator.clone())?;

    let latest_query_id = LATEST_QUERY_ID.load(deps.storage)?;
    let pool_delegation_query_id = latest_query_id + 1;
    let pool_query_id = latest_query_id + 2;
    let withdraw_query_id = latest_query_id + 3;

    let register_delegation_query_msg = new_register_delegator_delegations_query_msg(
        connection_id.clone(),
        delegator.clone(),
        validator_addrs,
        DEFAULT_UPDATE_PERIOD
    )?;

    // wrap into submessage to save {query_id, query_type} on reply that'll later be used to handle sudo kv callback
    let register_delegation_query_submsg = SubMsg::reply_on_success(
        register_delegation_query_msg,
        pool_delegation_query_id
    );

    let register_balance_pool_msg = new_register_balance_query_msg(
        connection_id.clone(),
        delegator.clone(),
        pool_info.remote_denom.clone(),
        DEFAULT_UPDATE_PERIOD
    )?;

    // wrap into submessage to save {query_id, query_type} on reply that'll later be used to handle sudo kv callback
    let register_balance_pool_submsg = SubMsg::reply_on_success(
        register_balance_pool_msg,
        pool_query_id
    );

    ADDR_QUERY_ID.save(deps.storage, delegator.clone(), &pool_query_id)?;

    let register_balance_withdraw_msg = new_register_balance_query_msg(
        connection_id.clone(),
        withdraw_addr.clone(),
        withdraw_addr.clone(),
        DEFAULT_UPDATE_PERIOD
    )?;

    // wrap into submessage to save {query_id, query_type} on reply that'll later be used to handle sudo kv callback
    let register_balance_withdraw_submsg = SubMsg::reply_on_success(
        register_balance_withdraw_msg,
        withdraw_query_id
    );

    ADDR_QUERY_ID.save(deps.storage, withdraw_addr.clone(), &withdraw_query_id)?;

    let set_withdraw_msg = MsgSetWithdrawAddress {
        delegator_address: delegator,
        withdraw_address: withdraw_addr,
    };
    let mut buf = Vec::new();
    buf.reserve(set_withdraw_msg.encoded_len());

    if let Err(e) = set_withdraw_msg.encode(&mut buf) {
        return Err(NeutronError::Std(StdError::generic_err(format!("Encode error: {}", e))));
    }

    let any_msg = ProtobufAny {
        type_url: "/cosmos.distribution.v1beta1.Msg/SetWithdrawAddress".to_string(),
        value: Binary::from(buf),
    };

    let cosmos_msg = NeutronMsg::submit_tx(
        connection_id.clone(),
        interchain_account_id.clone(),
        vec![any_msg],
        "".to_string(),
        DEFAULT_TIMEOUT_SECONDS,
        fee.clone()
    );

    // We use a submessage here because we need the process message reply to save
    // the outgoing IBC packet identifier for later.
    let submsg_set_withdraw = msg_with_sudo_callback(
        deps.branch(),
        cosmos_msg,
        SudoPayload::HandlerPayloadInterTx(InterTxType {
            port_id: get_port_id(env.contract.address.to_string(), interchain_account_id),
            // Here you can store some information about the transaction to help you parse
            // the acknowledgement later.
            message: "set_delegator_withdraw_addr".to_string(),
        })
    )?;

    pool_info.minimal_stake = minimal_stake;
    pool_info.rtoken = rtoken;
    pool_info.next_unstake_index = next_unstake_index;
    pool_info.unbonding_period = unbonding_period;
    pool_info.unstake_times_limit = unstake_times_limit;

    POOLS.save(deps.storage, pool_info.pool_addr.clone(), &pool_info)?;

    Ok(
        Response::new().add_submessages(
            vec![
                register_delegation_query_submsg,
                register_balance_pool_submsg,
                register_balance_withdraw_submsg,
                submsg_set_withdraw
            ]
        )
    )
}

pub fn register_delegations_query(
    connection_id: String,
    delegator: String,
    validators: Vec<String>,
    update_period: u64
) -> NeutronResult<Response<NeutronMsg>> {
    let msg = new_register_delegator_delegations_query_msg(
        connection_id,
        delegator,
        validators,
        update_period
    )?;

    Ok(Response::new().add_message(msg))
}

pub fn register_balance_query(
    connection_id: String,
    addr: String,
    denom: String,
    update_period: u64
) -> NeutronResult<Response<NeutronMsg>> {
    let msg = new_register_balance_query_msg(connection_id, addr, denom, update_period)?;

    Ok(Response::new().add_message(msg))
}

fn execute_stake(
    deps: DepsMut<NeutronQuery>,
    _: Env,
    neutron_address: String,
    pool_addr: String,
    info: MessageInfo
) -> NeutronResult<Response<NeutronMsg>> {
    let pool_denom = POOL_DENOM_MPA.load(deps.storage, pool_addr.clone())?;
    let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    let mut amount = 0;
    if !info.funds.is_empty() {
        amount = u128::from(
            info.funds
                .iter()
                .find(|c| c.denom == pool_denom)
                .map(|c| c.amount)
                .unwrap_or(Uint128::zero())
        );
    }

    amount = amount.mul(pool_info.rate.u128()).div(1_000_000);

    let msg = WasmMsg::Execute {
        contract_addr: pool_info.rtoken.to_string(),
        msg: to_json_binary(
            &(rtoken::msg::ExecuteMsg::Mint {
                recipient: neutron_address.to_string(),
                amount: Uint128::from(amount),
            })
        )?,
        funds: vec![],
    };

    Ok(Response::new().add_message(CosmosMsg::Wasm(msg)).add_attribute("mint", "call_contract_b"))
}

// Before this step, need the user to authorize burn from
fn execute_unstake(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    interchain_account_id: String,
    pool_addr: String
) -> NeutronResult<Response<NeutronMsg>> {
    if amount == Uint128::zero() {
        return Err(
            NeutronError::Std(
                StdError::generic_err(format!("Encode error: {}", "LSD token amount is zero"))
            )
        );
    }

    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    let unstake_count = UNSTAKES_INDEX_FOR_USER.load(deps.storage, &info.sender)?.len() as u128;
    let unstake_limit = pool_info.unstake_times_limit.u128();
    if unstake_count >= unstake_limit {
        return Err(
            NeutronError::Std(
                StdError::generic_err(format!("Encode error: {}", "Unstake times limit reached"))
            )
        );
    }

    // Calculate the number of tokens(atom)
    let token_amount = amount.mul(pool_info.rate);

    let (delegator, _) = get_ica(deps.as_ref(), &env, &interchain_account_id)?;

    // update pool info
    let mut pools = POOLS.load(deps.storage, pool_addr.clone())?;
    pools.unbond = pools.unbond.add(token_amount);
    // todo: Numerical check
    pools.active = pools.active.sub(token_amount);
    POOLS.save(deps.storage, pool_addr.clone(), &pools)?;

    // update unstake info
    let unstake_info = UnstakeInfo {
        era: pool_info.era,
        pool: delegator,
        amount: token_amount,
    };

    let will_use_unstake_index = pool_info.next_unstake_index;

    UNSTAKES_OF_INDEX.save(deps.storage, will_use_unstake_index.u128(), &unstake_info)?;

    pool_info.next_unstake_index = pool_info.next_unstake_index.add(Uint128::one());
    POOLS.save(deps.storage, pool_addr.clone(),&pool_info)?;

    // burn
    let msg = WasmMsg::Execute {
        contract_addr: pool_info.rtoken.to_string(),
        msg: to_json_binary(
            &(rtoken::msg::ExecuteMsg::BurnFrom {
                owner: info.sender.to_string(),
                amount: Default::default(),
            })
        )?,
        funds: vec![],
    };

    // send event
    Ok(
        Response::new()
            .add_message(CosmosMsg::Wasm(msg))
            .add_attribute("action", "unstake")
            .add_attribute("from", info.sender)
            .add_attribute("token_amount", token_amount.to_string())
            .add_attribute("lsd_token_amount", amount.to_string())
            .add_attribute("unstake_index", will_use_unstake_index.to_string())
    )
}

fn execute_withdraw(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    pool_addr: String,
    receiver: Addr,
    interchain_account_id: String
) -> NeutronResult<Response<NeutronMsg>> {
    let mut total_withdraw_amount = Uint128::zero();
    let mut unstakes = UNSTAKES_INDEX_FOR_USER.load(deps.storage, &info.sender)?;

    let mut emit_unstake_index_list = vec![];
    let mut indices_to_remove = Vec::new();

    let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    for (i, unstake_index) in unstakes.iter().enumerate() {
        let unstake_info = UNSTAKES_OF_INDEX.load(deps.storage, unstake_index.u128())?;
        if
            unstake_info.era + pool_info.unbonding_period > pool_info.era ||
            unstake_info.pool != pool_addr
        {
            continue;
        }

        // Remove the unstake index element of info.sender from UNSTAKES_INDEX_FOR_USER
        total_withdraw_amount += unstake_info.amount;
        emit_unstake_index_list.push(*unstake_index);
        indices_to_remove.push(i);
    }

    // Reverse sort the indices to remove to avoid shifting issues during removal
    indices_to_remove.sort_unstable_by(|a, b| b.cmp(a));

    // Remove the elements
    for index in indices_to_remove {
        unstakes.remove(index);
    }

    UNSTAKES_INDEX_FOR_USER.save(deps.storage, &info.sender, &unstakes)?;

    if total_withdraw_amount.is_zero() {
        return Err(
            NeutronError::Std(
                StdError::generic_err(format!("Encode error: {}", "Zero withdraw amount"))
            )
        );
    }

    let unstake_index_list_str = emit_unstake_index_list
        .iter()
        .map(|index| index.u128().to_string())
        .collect::<Vec<String>>()
        .join(",");

    // interchain tx send atom
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let (delegator, connection_id) = get_ica(deps.as_ref(), &env, &interchain_account_id)?;
    let ica_send = MsgSend {
        from_address: delegator,
        to_address: receiver.to_string(),
        amount: Vec::from([
            Coin {
                denom: pool_info.ibc_denom,
                amount: total_withdraw_amount.to_string(),
            },
        ]),
    };
    let mut buf = Vec::new();
    buf.reserve(ica_send.encoded_len());

    if let Err(e) = ica_send.encode(&mut buf) {
        return Err(NeutronError::Std(StdError::generic_err(format!("Encode error: {}", e))));
    }

    let send_msg = ProtobufAny {
        type_url: "/cosmos.bank.v1beta1.MsgSend".to_string(),
        value: Binary::from(buf),
    };

    let cosmos_msg = NeutronMsg::submit_tx(
        connection_id,
        interchain_account_id.clone(),
        vec![send_msg],
        "".to_string(),
        DEFAULT_TIMEOUT_SECONDS,
        fee
    );

    // We use a submessage here because we need the process message reply to save
    // the outgoing IBC packet identifier for later.
    let submsg = msg_with_sudo_callback(
        deps.branch(),
        cosmos_msg,
        SudoPayload::HandlerPayloadInterTx(InterTxType {
            port_id: get_port_id(env.contract.address.as_str(), &interchain_account_id),
            message: "message".to_string(),
        })
    )?;

    Ok(
        Response::new()
            .add_attribute("action", "withdraw")
            .add_attribute("from", info.sender)
            .add_attribute("pool", pool_addr.clone())
            .add_attribute("unstake_index_list", unstake_index_list_str)
            .add_attribute("amount", total_withdraw_amount)
            .add_submessages(vec![submsg])
    )
}

fn execute_stake_lsm(
    _: DepsMut<NeutronQuery>,
    _: Env,
    _: MessageInfo
) -> NeutronResult<Response<NeutronMsg>> {
    // todo!
    Ok(Response::new())
}

fn execute_rm_pool_validators(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    _: MessageInfo,
    pool_addr: String,
    validator_addrs: Vec<String>
) -> NeutronResult<Response<NeutronMsg>> {
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);

    // redelegate
    let registered_query_id = ADDR_QUERY_ID.load(deps.storage, pool_addr.clone())?;
    let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    let interchain_account_id = POOL_ICA_MAP.load(deps.storage, pool_addr.clone())?;
    // get info about the query
    let registered_query = get_registered_query(deps.as_ref(), registered_query_id)?;
    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Delegations structure from raw KV-storage values
    let delegations: Delegations = query_kv_result(deps.as_ref(), registered_query_id)?;

    let target_validator = find_redelegation_target(&delegations, &validator_addrs).unwrap();

    let mut msgs = vec![];

    for src_validator in validator_addrs {
        let amount = find_validator_amount(&delegations, src_validator.clone()).unwrap();
        // add submessage to unstake
        let redelegate_msg = MsgBeginRedelegate {
            delegator_address: pool_addr.clone(),
            validator_src_address: src_validator.clone(),
            validator_dst_address: target_validator.clone(),
            amount: Some(Coin {
                denom: pool_info.ibc_denom.clone(),
                amount: amount.to_string(),
            }),
        };
        let mut buf = Vec::new();
        buf.reserve(redelegate_msg.encoded_len());

        if let Err(e) = redelegate_msg.encode(&mut buf) {
            return Err(NeutronError::Std(StdError::generic_err(format!("Encode error: {}", e))));
        }

        let any_msg = ProtobufAny {
            type_url: "/cosmos.staking.v1beta1.MsgUndelegate".to_string(),
            value: Binary::from(buf),
        };

        let cosmos_msg = NeutronMsg::submit_tx(
            pool_info.connection_id.clone(),
            interchain_account_id.clone(),
            vec![any_msg],
            "".to_string(),
            DEFAULT_TIMEOUT_SECONDS,
            fee.clone()
        );

        // We use a submessage here because we need the process message reply to save
        // the outgoing IBC packet identifier for later.
        let submsg_redelegate = msg_with_sudo_callback(
            deps.branch(),
            cosmos_msg,
            SudoPayload::HandlerPayloadInterTx(InterTxType {
                port_id: get_port_id(
                    env.contract.address.to_string(),
                    interchain_account_id.clone()
                ),
                // Here you can store some information about the transaction to help you parse
                // the acknowledgement later.
                message: "interchain_undelegate".to_string(),
            })
        )?;
        msgs.push(submsg_redelegate);
    }

    // todo: update state in sudo reply
    // todo: update delegation_query
    Ok(Response::default().add_submessages(msgs))
}

fn execute_era_update(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    channel: String,
    pool_addr: String
) -> NeutronResult<Response<NeutronMsg>> {
    // --------------------------------------------------------------------------------------------------
    // contract must pay for relaying of acknowledgements
    // See more info here: https://docs.neutron.org/neutron/feerefunder/overview
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let mut msgs = vec![];
    let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    // check era state
    if pool_info.era_update_status != EraUpdated {
        deps.as_ref().api.debug(
            format!("WASMDEBUG: execute_era_update skip pool: {:?}", pool_addr).as_str()
        );
        return Ok(Response::new());
    }

    let balance = deps.querier.query_all_balances(&env.contract.address)?;

    // funds use contract funds
    let mut amount = 0;
    if !balance.is_empty() {
        amount = u128::from(
            balance
                .iter()
                .find(|c| c.denom == pool_info.ibc_denom.clone())
                .map(|c| c.amount)
                .unwrap_or(Uint128::zero())
        );
    }

    let tx_coin = coin(amount, pool_info.ibc_denom.clone());

    let msg = NeutronMsg::IbcTransfer {
        source_port: "transfer".to_string(),
        source_channel: channel.clone(),
        sender: env.contract.address.to_string(),
        receiver: pool_addr.clone(),
        token: tx_coin,
        timeout_height: RequestPacketTimeoutHeight {
            // todo: revision_number to pool_info?
            revision_number: Some(2),
            revision_height: Some(DEFAULT_TIMEOUT_HEIGHT),
        },
        timeout_timestamp: DEFAULT_TIMEOUT_SECONDS,
        memo: "".to_string(),
        fee: fee.clone(),
    };

    deps.as_ref().api.debug(format!("WASMDEBUG: IbcTransfer msg: {:?}", msg).as_str());

    let submsg_pool_ibc_send = msg_with_sudo_callback(
        deps.branch(),
        msg,
        SudoPayload::HandlerPayloadIbcSend(IbcSendType {
            message: "message".to_string(),
        })
    )?;
    deps.as_ref().api.debug(
        format!("WASMDEBUG: execute_send: sent submsg: {:?}", submsg_pool_ibc_send).as_str()
    );
    msgs.push(submsg_pool_ibc_send);

    // check withdraw address balance and send it to the pool
    let deps_as_ref = deps.as_ref();

    let query_id = ADDR_QUERY_ID.load(deps.storage, pool_info.withdraw_addr.clone())?;
    let registered_query = get_registered_query(deps_as_ref, query_id)?;
    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Balances structure from raw KV-storage values
    let withdraw_balances: Balances = query_kv_result(deps_as_ref, query_id)?;

    let mut withdraw_amount = 0;
    if !withdraw_balances.coins.is_empty() {
        withdraw_amount = u128::from(
            balance
                .iter()
                .find(|c| c.denom == pool_info.ibc_denom.clone())
                .map(|c| c.amount)
                .unwrap_or(Uint128::zero())
        );
    }

    let tx_withdraw_coin = coin(withdraw_amount, pool_info.ibc_denom.clone());
    let withdraw_token_send = NeutronMsg::IbcTransfer {
        source_port: "transfer".to_string(),
        source_channel: channel.clone(),
        sender: env.contract.address.to_string(),
        receiver: pool_addr.clone(),
        token: tx_withdraw_coin,
        timeout_height: RequestPacketTimeoutHeight {
            // todo: revision_number to pool_info?
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

    let submsg_withdraw_ibc_send = msg_with_sudo_callback(
        deps.branch(),
        withdraw_token_send,
        SudoPayload::HandlerPayloadIbcSend(IbcSendType {
            message: "message".to_string(),
        })
    )?;
    deps.as_ref().api.debug(
        format!("WASMDEBUG: execute_send: sent submsg: {:?}", submsg_withdraw_ibc_send).as_str()
    );
    msgs.push(submsg_withdraw_ibc_send);

    // todo: calu need withdraw --> use unstake index map

    Ok(Response::default().add_submessages(msgs))
}

fn execute_era_bond(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    pool_addr: String
) -> NeutronResult<Response<NeutronMsg>> {
    // --------------------------------------------------------------------------------------------------
    // contract must pay for relaying of acknowledgements
    // See more info here: https://docs.neutron.org/neutron/feerefunder/overview
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let mut msgs = vec![];
    let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    // check era state
    if pool_info.era_update_status != BondReported {
        deps.as_ref().api.debug(
            format!("WASMDEBUG: execute_era_bond skip pool: {:?}", pool_addr).as_str()
        );
        return Ok(Response::new());
    }

    let interchain_account_id = POOL_ICA_MAP.load(deps.storage, pool_addr.clone())?;
    if pool_info.unbond > pool_info.active {
        let unbond_amount = pool_info.unbond - pool_info.active;

        // get info about the query
        let registered_query_id = ADDR_QUERY_ID.load(deps.storage, pool_addr.clone())?;
        let deps_as_ref = deps.as_ref();
        let registered_query = get_registered_query(deps_as_ref, registered_query_id)?;
        // check that query type is KV
        check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
        // reconstruct a nice Delegations structure from raw KV-storage values
        let delegations: Delegations = query_kv_result(deps_as_ref, registered_query_id)?;

        let unbond_infos = allocate_unbond_amount(&delegations, unbond_amount);
        for info in unbond_infos {
            println!(
                "Validator: {}, Delegation: {}, Unbond: {}",
                info.validator,
                info.delegation_amount,
                info.unbond_amount
            );

            // add submessage to unstake
            let delegate_msg = MsgUndelegate {
                delegator_address: pool_addr.clone(),
                validator_address: info.validator.clone(),
                amount: Some(Coin {
                    denom: pool_info.ibc_denom.clone(),
                    amount: info.unbond_amount.to_string(),
                }),
            };
            let mut buf = Vec::new();
            buf.reserve(delegate_msg.encoded_len());

            if let Err(e) = delegate_msg.encode(&mut buf) {
                return Err(
                    NeutronError::Std(StdError::generic_err(format!("Encode error: {}", e)))
                );
            }

            let any_msg = ProtobufAny {
                type_url: "/cosmos.staking.v1beta1.MsgUndelegate".to_string(),
                value: Binary::from(buf),
            };

            let cosmos_msg = NeutronMsg::submit_tx(
                pool_info.connection_id.clone(),
                interchain_account_id.clone(),
                vec![any_msg],
                "".to_string(),
                DEFAULT_TIMEOUT_SECONDS,
                fee.clone()
            );

            // We use a submessage here because we need the process message reply to save
            // the outgoing IBC packet identifier for later.
            let submsg_unstake = msg_with_sudo_callback(
                deps.branch(),
                cosmos_msg,
                SudoPayload::HandlerPayloadInterTx(InterTxType {
                    port_id: get_port_id(
                        env.contract.address.to_string(),
                        interchain_account_id.clone()
                    ),
                    // Here you can store some information about the transaction to help you parse
                    // the acknowledgement later.
                    message: "interchain_undelegate".to_string(),
                })
            )?;

            msgs.push(submsg_unstake);
        }
    } else if pool_info.active > pool_info.need_withdraw {
        let stake_amount = pool_info.active - pool_info.need_withdraw;

        let validator_count = pool_info.validator_addrs.len() as u128;

        if validator_count == 0 {
            return Err(NeutronError::Std(StdError::generic_err("validator_count is zero")));
        }

        let amount_per_validator = stake_amount.div(Uint128::from(validator_count));
        let remainder = stake_amount.sub(amount_per_validator.mul(amount_per_validator));

        for (index, validator_addr) in pool_info.validator_addrs.iter().enumerate() {
            let mut amount_for_this_validator = amount_per_validator;

            // Add the remainder to the first validator
            if index == 0 {
                amount_for_this_validator += remainder;
            }

            // add submessage to stake
            let delegate_msg = MsgDelegate {
                delegator_address: pool_addr.clone(),
                validator_address: validator_addr.clone(),
                amount: Some(Coin {
                    denom: pool_info.ibc_denom.clone(),
                    amount: amount_for_this_validator.to_string(),
                }),
            };

            // Serialize the Delegate message.
            let mut buf = Vec::new();
            buf.reserve(delegate_msg.encoded_len());

            if let Err(e) = delegate_msg.encode(&mut buf) {
                return Err(
                    NeutronError::Std(StdError::generic_err(format!("Encode error: {}", e)))
                );
            }

            // Put the serialized Delegate message to a types.Any protobuf message.
            let any_msg = ProtobufAny {
                type_url: "/cosmos.staking.v1beta1.MsgDelegate".to_string(),
                value: Binary::from(buf),
            };

            // Form the neutron SubmitTx message containing the binary Delegate message.
            let cosmos_msg = NeutronMsg::submit_tx(
                pool_info.connection_id.clone(),
                interchain_account_id.clone(),
                vec![any_msg],
                "".to_string(),
                DEFAULT_TIMEOUT_SECONDS,
                fee.clone()
            );

            // We use a submessage here because we need the process message reply to save
            // the outgoing IBC packet identifier for later.
            let submsg_stake = msg_with_sudo_callback(
                deps.branch(),
                cosmos_msg,
                SudoPayload::HandlerPayloadInterTx(InterTxType {
                    port_id: get_port_id(
                        env.contract.address.to_string(),
                        interchain_account_id.clone()
                    ),
                    // Here you can store some information about the transaction to help you parse
                    // the acknowledgement later.
                    message: "interchain_delegate".to_string(),
                })
            )?;
            msgs.push(submsg_stake);
        }
    }

    Ok(Response::default().add_submessages(msgs))
}

fn execute_bond_active(
    deps: DepsMut<NeutronQuery>,
    _: Env,
    pool_addr: String
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    // check era state
    if pool_info.era_update_status != BondReported {
        deps.as_ref().api.debug(
            format!("WASMDEBUG: execute_era_bond skip pool: {:?}", pool_addr).as_str()
        );
        return Ok(Response::default());
    }
    let registered_query_id = ADDR_QUERY_ID.load(deps.storage, pool_addr.clone())?;

    // get info about the query
    let deps_as_ref = deps.as_ref();
    let registered_query = get_registered_query(deps_as_ref, registered_query_id)?;
    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Delegations structure from raw KV-storage values
    let delegations: Delegations = query_kv_result(deps_as_ref, registered_query_id)?;

    let mut total_amount = cosmwasm_std::Coin {
        denom: pool_info.remote_denom.clone(),
        amount: Uint128::zero(),
    };

    for delegation in delegations.delegations {
        total_amount.amount = total_amount.amount.add(delegation.amount.amount);
    }

    let token_info_msg = rtoken::msg::QueryMsg::TokenInfo {};
    let token_info: cw20::TokenInfoResponse = deps.querier.query(
        &QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: pool_info.rtoken.to_string(),
            msg: to_json_binary(&token_info_msg)?,
        })
    )?;
    // todo: calculate protocol fee
    pool_info.rate = total_amount.amount.div(token_info.total_supply);
    pool_info.era_update_status = PoolBondState::ActiveReported;
    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;

    Ok(Response::default())
}

#[entry_point]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> StdResult<Response> {
    deps.api.debug(format!("WASMDEBUG: reply msg: {:?}", msg).as_str());
    match msg.id {
        // It's convenient to use range of ID's to handle multiple reply messages
        IBC_SUDO_ID_RANGE_START..=IBC_SUDO_ID_RANGE_END => prepare_sudo_payload(deps, env, msg),
        _ => Err(StdError::generic_err(format!("unsupported reply message id {}", msg.id))),
    }
}

// todo: update pool era state
#[entry_point]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> StdResult<Response> {
    deps.api.debug(format!("WASMDEBUG: sudo: received sudo msg: {:?}", msg).as_str());

    match msg {
        // For handling kv query result
        // For handling successful (non-error) acknowledgements
        SudoMsg::Response { request, data } => sudo_response(deps, request, data),

        // For handling error acknowledgements
        SudoMsg::Error { request, details } => sudo_error(deps, request, details),

        // For handling error timeouts
        SudoMsg::Timeout { request } => sudo_timeout(deps, request),

        // For handling successful registering of ICA
        SudoMsg::OpenAck { port_id, channel_id, counterparty_channel_id, counterparty_version } =>
            sudo_open_ack(
                deps,
                env,
                port_id,
                channel_id,
                counterparty_channel_id,
                counterparty_version
            ),

        SudoMsg::KVQueryResult { query_id } => sudo_kv_query_result(deps, env, query_id),
        _ => Ok(Response::default()),
    }
}

/// sudo_kv_query_result is the contract's callback for KV query results. Note that only the query
/// id is provided, so you need to read the query result from the state.
pub fn sudo_kv_query_result(deps: DepsMut, _env: Env, query_id: u64) -> StdResult<Response> {
    deps.api.debug(
        format!("WASMDEBUG: sudo_kv_query_result received; query_id: {:?}", query_id).as_str()
    );

    KV_QUERY_ID_TO_CALLBACKS.save(deps.storage, query_id, &QueryKind::Balance)?;

    Ok(Response::default())
}

// a callback handler for payload of IbcSendType
fn sudo_ibc_send_callback(deps: Deps, payload: IbcSendType) -> StdResult<Response> {
    deps.api.debug(format!("WASMDEBUG: callback: ibc send sudo payload: {:?}", payload).as_str());
    Ok(Response::new())
}

// a callback handler for payload of InterTxType
fn sudo_inter_tx_callback(deps: Deps, payload: InterTxType) -> StdResult<Response> {
    deps.api.debug(format!("WASMDEBUG: callback: inter tx sudo payload: {:?}", payload).as_str());
    Ok(Response::new())
}

// saves payload to process later to the storage and returns a SubmitTX Cosmos SubMsg with necessary reply id
fn msg_with_sudo_callback<C: Into<CosmosMsg<T>>, T>(
    deps: DepsMut<NeutronQuery>,
    msg: C,
    payload: SudoPayload
) -> StdResult<SubMsg<T>> {
    let id = save_reply_payload(deps.storage, payload)?;
    Ok(SubMsg::reply_on_success(msg, id))
}

// prepare_sudo_payload is called from reply handler
// The method is used to extract sequence id and channel from SubmitTxResponse to process sudo payload defined in msg_with_sudo_callback later in Sudo handler.
// Such flow msg_with_sudo_callback() -> reply() -> prepare_sudo_payload() -> sudo() allows you "attach" some payload to your Transfer message
// and process this payload when an acknowledgement for the SubmitTx message is received in Sudo handler
fn prepare_sudo_payload(mut deps: DepsMut, _env: Env, msg: Reply) -> StdResult<Response> {
    let payload = read_reply_payload(deps.storage, msg.id)?;
    let resp: MsgIbcTransferResponse = from_json(
        msg.result
            .into_result()
            .map_err(StdError::generic_err)?
            .data.ok_or_else(|| StdError::generic_err("no result"))?
    ).map_err(|e| StdError::generic_err(format!("failed to parse response: {:?}", e)))?;
    let seq_id = resp.sequence_id;
    let channel_id = resp.channel;
    save_sudo_payload(deps.branch().storage, channel_id, seq_id, payload)?;
    Ok(Response::new())
}

fn sudo_error(deps: DepsMut, req: RequestPacket, data: String) -> StdResult<Response> {
    deps.api.debug(
        format!("WASMDEBUG: sudo_error: sudo error received: {:?} {}", req, data).as_str()
    );
    Ok(Response::new())
}

fn sudo_timeout(deps: DepsMut, req: RequestPacket) -> StdResult<Response> {
    deps.api.debug(
        format!("WASMDEBUG: sudo_timeout: sudo timeout ack received: {:?}", req).as_str()
    );
    Ok(Response::new())
}

fn sudo_response(deps: DepsMut, req: RequestPacket, data: Binary) -> StdResult<Response> {
    deps.api.debug(format!("WASMDEBUG: sudo_response: sudo received: {:?} {}", req, data).as_str());
    let seq_id = req.sequence.ok_or_else(|| StdError::generic_err("sequence not found"))?;
    let channel_id = req.source_channel.ok_or_else(||
        StdError::generic_err("channel_id not found")
    )?;

    match read_sudo_payload(deps.storage, channel_id, seq_id)? {
        SudoPayload::HandlerPayloadIbcSend(t) => sudo_ibc_send_callback(deps.as_ref(), t),
        SudoPayload::HandlerPayloadInterTx(t) => sudo_inter_tx_callback(deps.as_ref(), t),
    }
    // at this place we can safely remove the data under (channel_id, seq_id) key
    // but it costs an extra gas, so its on you how to use the storage
}

// handler
fn sudo_open_ack(
    deps: DepsMut,
    _env: Env,
    port_id: String,
    _channel_id: String,
    _counterparty_channel_id: String,
    counterparty_version: String
) -> StdResult<Response> {
    // The version variable contains a JSON value with multiple fields,
    // including the generated account address.
    let parsed_version: Result<OpenAckVersion, _> = serde_json_wasm::from_str(
        counterparty_version.as_str()
    );

    // Update the storage record associated with the interchain account.
    if let Ok(parsed_version) = parsed_version {
        INTERCHAIN_ACCOUNTS.save(
            deps.storage,
            port_id,
            &Some((parsed_version.address.clone(), parsed_version.controller_connection_id.clone()))
        )?;
        POOL_ICA_MAP.save(
            deps.storage,
            parsed_version.address,
            &parsed_version.controller_connection_id
        )?;
        return Ok(Response::default());
    }
    Err(StdError::generic_err("Can't parse counterparty_version"))
}

fn min_ntrn_ibc_fee(fee: IbcFee) -> IbcFee {
    IbcFee {
        recv_fee: fee.recv_fee,
        ack_fee: fee.ack_fee
            .into_iter()
            .filter(|a| a.denom == FEE_DENOM)
            .collect(),
        timeout_fee: fee.timeout_fee
            .into_iter()
            .filter(|a| a.denom == FEE_DENOM)
            .collect(),
    }
}

fn get_ica(
    deps: Deps<impl CustomQuery>,
    env: &Env,
    interchain_account_id: &str
) -> Result<(String, String), StdError> {
    let key = get_port_id(env.contract.address.as_str(), interchain_account_id);

    INTERCHAIN_ACCOUNTS.load(deps.storage, key)?.ok_or_else(||
        StdError::generic_err("Interchain account is not created yet")
    )
}

fn allocate_unbond_amount(
    delegations: &Delegations,
    unbond_amount: Uint128
) -> Vec<ValidatorUnbondInfo> {
    let mut unbond_infos: Vec<ValidatorUnbondInfo> = Vec::new();
    let mut remaining_unbond = unbond_amount;

    // Sort the delegations by amount in descending order
    let mut sorted_delegations = delegations.delegations.clone();
    sorted_delegations.sort_by(|a, b| b.amount.amount.cmp(&a.amount.amount));

    for delegation in sorted_delegations.iter() {
        if remaining_unbond.is_zero() {
            break;
        }

        let mut current_unbond = remaining_unbond;

        // If the current validator delegate amount is less than the remaining delegate amount, all are discharged
        if delegation.amount.amount < remaining_unbond {
            current_unbond = delegation.amount.amount;
        }

        remaining_unbond -= current_unbond;

        unbond_infos.push(ValidatorUnbondInfo {
            validator: delegation.validator.clone(),
            delegation_amount: delegation.amount.amount,
            unbond_amount: current_unbond,
        });
    }

    unbond_infos
}

fn find_redelegation_target(
    delegations: &Delegations,
    excluded_validators: &[String]
) -> Option<String> {
    // Find the validator from delegations that is not in excluded_validators and has the smallest delegate count
    let mut min_delegation: Option<(String, Uint128)> = None;

    for delegation in &delegations.delegations {
        // Skip the validators in excluded_validators
        if excluded_validators.contains(&delegation.validator) {
            continue;
        }

        // Update the minimum delegation validator
        match min_delegation {
            Some((_, min_amount)) if delegation.amount.amount < min_amount => {
                min_delegation = Some((delegation.validator.clone(), delegation.amount.amount));
            }
            None => {
                min_delegation = Some((delegation.validator.clone(), delegation.amount.amount));
            }
            _ => {}
        }
    }

    min_delegation.map(|(validator, _)| validator)
}

fn find_validator_amount(delegations: &Delegations, validator_address: String) -> Option<Uint128> {
    for delegation in &delegations.delegations {
        if delegation.validator == validator_address {
            return Some(delegation.amount.amount);
        }
    }
    None
}

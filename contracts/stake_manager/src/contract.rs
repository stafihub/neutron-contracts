use std::ops::{ Add, Div, Mul, Sub };
use std::vec;

use cosmos_sdk_proto::cosmos::{ bank::v1beta1::MsgSend, staking::v1beta1::MsgBeginRedelegate };
use cosmos_sdk_proto::cosmos::base::v1beta1::Coin;
use cosmos_sdk_proto::cosmos::distribution::v1beta1::MsgSetWithdrawAddress;
use cosmos_sdk_proto::cosmos::staking::v1beta1::{ MsgDelegate, MsgUndelegate };
use cosmos_sdk_proto::prost::Message;
use cosmwasm_std::{
    Addr,
    Binary,
    coin,
    CosmosMsg,
    CustomQuery,
    Deps,
    DepsMut,
    entry_point,
    Env,
    from_json,
    MessageInfo,
    QueryRequest,
    Reply,
    Response,
    StdError,
    StdResult,
    SubMsg,
    to_json_binary,
    Uint128,
    WasmMsg,
    WasmQuery,
    Order,
};
use cw2::set_contract_version;
use neutron_sdk::{
    bindings::{
        msg::{ IbcFee, MsgIbcTransferResponse, NeutronMsg },
        query::{ NeutronQuery, QueryInterchainAccountAddressResponse },
    },
    interchain_queries::{
        check_query_type,
        get_registered_query,
        query_kv_result,
        types::QueryType,
        v045::{ queries::BalanceResponse, types::Balances, types::Delegations },
    },
    NeutronError,
    NeutronResult,
    query::min_ibc_fee::query_min_ibc_fee,
    sudo::msg::{ RequestPacket, RequestPacketTimeoutHeight },
};
use neutron_sdk::bindings::msg::MsgRegisterInterchainQueryResponse;
use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::interchain_queries::v045::new_register_balance_query_msg;
use neutron_sdk::interchain_queries::v045::new_register_delegator_delegations_query_msg;
use neutron_sdk::interchain_txs::helpers::get_port_id;
use neutron_sdk::sudo::msg::SudoMsg;
use schemars::JsonSchema;
use serde::{ Deserialize, Serialize };

use crate::msg::ConfigPoolParams;
use crate::{
    msg::{ ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg },
    state::{
        ACKNOWLEDGEMENT_RESULTS,
        ADDR_QUERY_ID,
        IBC_SUDO_ID_RANGE_END,
        IBC_SUDO_ID_RANGE_START,
        KV_QUERY_ID_TO_CALLBACKS,
        LATEST_BALANCES_QUERY_ID,
        LATEST_DELEGATIONS_QUERY_ID,
        PoolBondState,
        PoolInfo,
        QUERY_BALANCES_REPLY_ID_END,
        QUERY_BALANCES_REPLY_ID_RANGE_START,
        QUERY_DELEGATIONS_REPLY_ID_END,
        QUERY_DELEGATIONS_REPLY_ID_RANGE_START,
        QueryKind,
        read_errors_from_queue,
        read_reply_payload,
        read_sudo_payload,
        save_reply_payload,
        save_sudo_payload,
    },
};
use crate::state::{
    INTERCHAIN_ACCOUNTS,
    OWN_QUERY_ID_TO_ICQ_ID,
    POOL_ICA_MAP,
    POOLS,
    STATE,
    State,
    UnstakeInfo,
    UNSTAKES_INDEX_FOR_USER,
    UNSTAKES_OF_INDEX,
};
use crate::state::PoolBondState::{ BondReported, EraUpdated, ActiveReported };

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
pub enum TxType {
    Default,
    SetWithdrawAddr,
    RmValidator,
    UserWithdraw,
    EraUpdate,
    EraUpdateIbcSend,
    EraUpdateWithdrawSend,
    EraBondStake,
    EraBondUnstake,
    EraActive,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InterTxType {
    pub message: String,
    pub port_id: String,
    pub tx_type: TxType,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct SudoPayload {
    pub message: String,
    pub port_id: String,
    pub tx_type: TxType,
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

    LATEST_BALANCES_QUERY_ID.save(deps.storage, &QUERY_BALANCES_REPLY_ID_RANGE_START)?;
    LATEST_DELEGATIONS_QUERY_ID.save(deps.storage, &QUERY_DELEGATIONS_REPLY_ID_RANGE_START)?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    deps.api.debug("WASMDEBUG: migrate");
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<NeutronQuery>, env: Env, msg: QueryMsg) -> NeutronResult<Binary> {
    deps.api.debug(format!("WASMDEBUG: query msg is {:?}", msg).as_str());

    match msg {
        QueryMsg::GetRegisteredQuery { query_id } => {
            Ok(to_json_binary(&get_registered_query(deps, query_id)?)?)
        }
        QueryMsg::Balance { query_id } => query_balance(deps, env, query_id),
        QueryMsg::PoolInfo { pool_addr } => query_pool_info(deps, env, pool_addr),
        QueryMsg::InterchainAccountAddress { interchain_account_id, connection_id } =>
            query_interchain_address(deps, env, interchain_account_id, connection_id),
        QueryMsg::InterchainAccountAddressFromContract { interchain_account_id } =>
            query_interchain_address_contract(deps, env, interchain_account_id),
        QueryMsg::AcknowledgementResult { interchain_account_id, sequence_id } =>
            query_acknowledgement_result(deps, env, interchain_account_id, sequence_id),
        QueryMsg::UserUnstake { pool_addr, user_neutron_addr } =>
            query_user_unstake(deps, pool_addr, user_neutron_addr),
        QueryMsg::ErrorsQueue {} => query_errors_queue(deps),
    }
}

pub fn query_user_unstake(
    deps: Deps<NeutronQuery>,
    pool_addr: String,
    user_neutron_addr: Addr
) -> NeutronResult<Binary> {
    let index_list = UNSTAKES_INDEX_FOR_USER.load(deps.storage, &user_neutron_addr)?;
    let mut results = vec![];
    for index in index_list {
        let unstake_info = UNSTAKES_OF_INDEX.load(deps.storage, index)?;
        if unstake_info.pool_addr != pool_addr {
            continue;
        }
        results.push(unstake_info);
    }
    Ok(to_json_binary(&results)?)
}

pub fn query_balance_by_addr(
    deps: Deps<NeutronQuery>,
    addr: String
) -> NeutronResult<BalanceResponse> {
    let contract_query_id = ADDR_QUERY_ID.load(deps.storage, addr)?;
    let registered_query_id = OWN_QUERY_ID_TO_ICQ_ID.load(deps.storage, contract_query_id)?;
    // get info about the query
    let registered_query = get_registered_query(deps, registered_query_id)?;
    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Balances structure from raw KV-storage values
    let balances: Balances = query_kv_result(deps, registered_query_id)?;

    deps.api.debug(format!("WASMDEBUG: query_balance_by_addr Balances is {:?}", balances).as_str());

    Ok(BalanceResponse {
        // last_submitted_height tells us when the query result was updated last time (block height)
        last_submitted_local_height: registered_query.registered_query.last_submitted_result_local_height,
        balances,
    })
}

pub fn query_delegation_by_addr(
    deps: Deps<NeutronQuery>,
    addr: String
) -> NeutronResult<Delegations> {
    let contract_query_id = ADDR_QUERY_ID.load(deps.storage, addr)?;
    let registered_query_id = OWN_QUERY_ID_TO_ICQ_ID.load(deps.storage, contract_query_id)?;
    // get info about the query
    let registered_query = get_registered_query(deps, registered_query_id)?;
    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Balances structure from raw KV-storage values
    let delegations: Delegations = query_kv_result(deps, registered_query_id)?;

    deps.api.debug(
        format!("WASMDEBUG: query_delegation_by_addr Delegations is {:?}", delegations).as_str()
    );

    Ok(delegations)
}

pub fn query_balance(
    deps: Deps<NeutronQuery>,
    _env: Env,
    registered_query_id: u64
) -> NeutronResult<Binary> {
    // get info about the query
    let registered_query = get_registered_query(deps, registered_query_id)?;
    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Balances structure from raw KV-storage values
    let balances: Balances = query_kv_result(deps, registered_query_id)?;

    deps.api.debug(format!("WASMDEBUG: query_balance Balances is {:?}", balances).as_str());

    Ok(
        to_json_binary(
            &(BalanceResponse {
                // last_submitted_height tells us when the query result was updated last time (block height)
                last_submitted_local_height: registered_query.registered_query.last_submitted_result_local_height,
                balances,
            })
        )?
    )
}

pub fn query_pool_info(
    deps: Deps<NeutronQuery>,
    _env: Env,
    pool_addr: String
) -> NeutronResult<Binary> {
    let pool_info = POOLS.load(deps.storage, pool_addr)?;

    Ok(to_json_binary(&pool_info)?)
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
    deps.as_ref().api.debug(format!("WASMDEBUG: execute msg is {:?}", msg).as_str());
    match msg {
        // NOTE: this is an example contract that shows how to make IBC transfers!
        // todo: Please add necessary authorization or other protection mechanisms
        // if you intend to send funds over IBC
        ExecuteMsg::RegisterPool { connection_id, interchain_account_id, register_fee } =>
            execute_register_pool(
                deps,
                env,
                info,
                connection_id,
                interchain_account_id,
                register_fee
            ),
        ExecuteMsg::ConfigPool(params) => { execute_config_pool(deps, env, info, *params) }
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
        ExecuteMsg::Unstake { amount, pool_addr } => execute_unstake(deps, info, amount, pool_addr),
        ExecuteMsg::Withdraw { pool_addr, receiver, interchain_account_id } =>
            execute_withdraw(deps, env, info, pool_addr, receiver, interchain_account_id),
        ExecuteMsg::PoolRmValidator { pool_addr, validator_addrs } =>
            execute_rm_pool_validators(deps, env, info, pool_addr, validator_addrs),
        ExecuteMsg::PoolAddValidator { pool_addr, validator_addrs } =>
            execute_add_pool_validators(deps, pool_addr, validator_addrs),
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
    interchain_account_id: String,
    register_fee: Vec<cosmwasm_std::Coin>
) -> NeutronResult<Response<NeutronMsg>> {
    deps.as_ref().api.debug(format!("WASMDEBUG: register_fee {:?}", register_fee).as_str());
    let register = NeutronMsg::register_interchain_account(
        connection_id.clone(),
        interchain_account_id.clone(),
        Some(register_fee)
    );

    deps.as_ref().api.debug(format!("WASMDEBUG: register msg is {:?}", register).as_str());

    let key = get_port_id(env.contract.address.as_str(), &interchain_account_id);

    deps.as_ref().api.debug(format!("WASMDEBUG: register key is {:?}", key).as_str());

    // we are saving empty data here because we handle response of registering ICA in sudo_open_ack method
    INTERCHAIN_ACCOUNTS.save(deps.storage, key, &None)?;

    Ok(Response::default().add_message(register))
}

// add execute to config the validator addrs and withdraw address on reply
fn execute_config_pool(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    _: MessageInfo,
    param: ConfigPoolParams
) -> NeutronResult<Response<NeutronMsg>> {
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let (delegator, connection_id) = get_ica(deps.as_ref(), &env, &param.interchain_account_id)?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_config_pool get_ica delegator: {:?},connection_id: {:?}",
            delegator,
            connection_id
        ).as_str()
    );

    let mut pool_info = POOLS.load(deps.as_ref().storage, delegator.clone())?;

    deps.as_ref().api.debug(
        format!("WASMDEBUG: execute_config_pool POOLS.load: {:?}", pool_info).as_str()
    );

    pool_info.need_withdraw = param.need_withdraw;
    pool_info.unbond = param.unbond;
    pool_info.active = param.active;
    pool_info.ibc_denom = param.ibc_denom;
    pool_info.remote_denom = param.remote_denom;
    pool_info.era = param.era;
    pool_info.rate = param.rate;
    pool_info.minimal_stake = param.minimal_stake;
    pool_info.rtoken = param.rtoken;
    pool_info.next_unstake_index = param.next_unstake_index;
    pool_info.unbonding_period = param.unbonding_period;
    pool_info.unstake_times_limit = param.unstake_times_limit;
    pool_info.connection_id = connection_id.clone();
    pool_info.validator_addrs = param.validator_addrs.clone(); // todo update pool_info validator_addrs in query replay
    pool_info.withdraw_addr = delegator.clone(); // todo: update withdraw addr in sudo reply
    pool_info.unbond_commission = param.unbond_commission;
    pool_info.protocol_fee_receiver = param.protocol_fee_receiver;

    POOLS.save(deps.storage, pool_info.pool_addr.clone(), &pool_info)?;

    let latest_balance_query_id = LATEST_BALANCES_QUERY_ID.load(deps.as_ref().storage)?;
    let latest_delegation_query_id = LATEST_DELEGATIONS_QUERY_ID.load(deps.as_ref().storage)?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_config_pool pool update: {:?},latest_query_id is {:?}",
            pool_info,
            latest_balance_query_id
        ).as_str()
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
        param.withdraw_addr.clone(),
        pool_info.remote_denom.clone(),
        DEFAULT_UPDATE_PERIOD
    )?;

    // wrap into submessage to save {query_id, query_type} on reply that'll later be used to handle sudo kv callback
    let register_balance_withdraw_submsg = SubMsg::reply_on_success(
        register_balance_withdraw_msg,
        withdraw_query_id
    );

    ADDR_QUERY_ID.save(deps.storage, param.withdraw_addr.clone(), &withdraw_query_id)?;

    let set_withdraw_msg = MsgSetWithdrawAddress {
        delegator_address: delegator.clone(),
        withdraw_address: param.withdraw_addr.clone(),
    };
    let mut buf = Vec::new();
    buf.reserve(set_withdraw_msg.encoded_len());

    if let Err(e) = set_withdraw_msg.encode(&mut buf) {
        return Err(NeutronError::Std(StdError::generic_err(format!("Encode error: {}", e))));
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
        fee.clone()
    );

    deps.as_ref().api.debug(
        format!("WASMDEBUG: execute_config_pool cosmos_msg is {:?}", cosmos_msg).as_str()
    );

    // We use a submessage here because we need the process message reply to save
    // the outgoing IBC packet identifier for later.
    let submsg_set_withdraw = msg_with_sudo_callback(deps.branch(), cosmos_msg, SudoPayload {
        port_id: get_port_id(env.contract.address.to_string(), param.interchain_account_id),
        message: format!("{}_{}", delegator, param.withdraw_addr),
        tx_type: TxType::SetWithdrawAddr,
    })?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_config_pool submsg_set_withdraw: {:?}",
            submsg_set_withdraw
        ).as_str()
    );

    Ok(
        Response::default().add_submessages(
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
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    let mut token_amount = 0;
    if !info.funds.is_empty() {
        token_amount = u128::from(
            info.funds
                .iter()
                .find(|c| c.denom == pool_info.ibc_denom.clone())
                .map(|c| c.amount)
                .unwrap_or(Uint128::zero())
        );
    }

    pool_info.active = pool_info.active.add(Uint128::new(token_amount));

    let rtoken_amount = token_amount.mul(pool_info.rate.u128()).div(1_000_000);

    let msg = WasmMsg::Execute {
        contract_addr: pool_info.rtoken.to_string(),
        msg: to_json_binary(
            &(rtoken::msg::ExecuteMsg::Mint {
                recipient: neutron_address.to_string(),
                amount: Uint128::from(rtoken_amount),
            })
        )?,
        funds: vec![],
    };

    POOLS.save(deps.storage, pool_addr, &pool_info)?;

    Ok(Response::new().add_message(CosmosMsg::Wasm(msg)).add_attribute("mint", "call_contract_b"))
}

// Before this step, need the user to authorize burn from
fn execute_unstake(
    deps: DepsMut<NeutronQuery>,
    info: MessageInfo,
    mut rtoken_amount: Uint128,
    pool_addr: String
) -> NeutronResult<Response<NeutronMsg>> {
    if rtoken_amount == Uint128::zero() {
        return Err(
            NeutronError::Std(
                StdError::generic_err(format!("Encode error: {}", "rtoken amount is zero"))
            )
        );
    }

    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    deps.as_ref().api.debug(
        format!("WASMDEBUG: execute_unstake pool_info: {:?}", pool_info).as_str()
    );

    let unstake_count = match UNSTAKES_INDEX_FOR_USER.load(deps.storage, &info.sender) {
        Ok(unstakes) => unstakes.len() as u128,
        Err(_) => 0u128,
    };

    deps.as_ref().api.debug(
        format!("WASMDEBUG: execute_unstake UNSTAKES_INDEX_FOR_USER: {:?}", unstake_count).as_str()
    );

    let unstake_limit = pool_info.unstake_times_limit.u128();
    if unstake_count >= unstake_limit {
        return Err(
            NeutronError::Std(
                StdError::generic_err(format!("Encode error: {}", "Unstake times limit reached"))
            )
        );
    }

    // Calculate the number of tokens(atom)
    let token_amount = rtoken_amount.mul(Uint128::new(1_000_000)).div(pool_info.rate);

    // cal fee
    let mut cms_fee = Uint128::zero();
    if pool_info.unbond_commission > Uint128::zero() {
        cms_fee = rtoken_amount.mul(pool_info.unbond_commission).div(Uint128::new(1_000_000));
        rtoken_amount = rtoken_amount.div(cms_fee);
    }
    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_unstake cms_fee: {:?} rtoken_amount: {:?}",
            cms_fee,
            rtoken_amount
        ).as_str()
    );

    // update pool info
    pool_info.unbond = pool_info.unbond.add(token_amount);
    pool_info.active = pool_info.active.sub(token_amount);

    // update unstake info
    let unstake_info = UnstakeInfo {
        era: pool_info.era,
        pool_addr: pool_addr.clone(),
        amount: token_amount,
    };

    let will_use_unstake_index = pool_info.next_unstake_index;
    pool_info.next_unstake_index = pool_info.next_unstake_index.add(Uint128::one());

    // burn
    let burn_msg = WasmMsg::Execute {
        contract_addr: pool_info.rtoken.to_string(),
        msg: to_json_binary(
            &(rtoken::msg::ExecuteMsg::BurnFrom {
                owner: info.sender.to_string(),
                amount: rtoken_amount,
            })
        )?,
        funds: vec![],
    };

    let send_fee = WasmMsg::Execute {
        contract_addr: pool_info.rtoken.to_string(),
        msg: to_json_binary(
            &(rtoken::msg::ExecuteMsg::TransferFrom {
                owner: info.sender.to_string(),
                recipient: pool_info.protocol_fee_receiver.to_string(),
                amount: cms_fee,
            })
        )?,
        funds: vec![],
    };

    UNSTAKES_OF_INDEX.save(deps.storage, will_use_unstake_index.u128(), &unstake_info)?;
    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;

    // send event
    Ok(
        Response::new()
            .add_message(CosmosMsg::Wasm(burn_msg))
            .add_message(CosmosMsg::Wasm(send_fee))
            .add_attribute("action", "unstake")
            .add_attribute("from", info.sender)
            .add_attribute("token_amount", token_amount.to_string())
            .add_attribute("rtoken_amount", rtoken_amount.to_string())
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
        let unstake_info = UNSTAKES_OF_INDEX.load(deps.storage, *unstake_index)?;
        if
            unstake_info.era + pool_info.unbonding_period > pool_info.era ||
            unstake_info.pool_addr != pool_addr
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
        .map(|index| index.to_string())
        .collect::<Vec<String>>()
        .join(",");

    // interchain tx send atom
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let ica_send = MsgSend {
        from_address: pool_addr.clone(),
        to_address: receiver.to_string(),
        amount: Vec::from([
            Coin {
                denom: pool_info.remote_denom,
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
        pool_info.connection_id.clone(),
        interchain_account_id.clone(),
        vec![send_msg],
        "".to_string(),
        DEFAULT_TIMEOUT_SECONDS,
        fee
    );

    // We use a submessage here because we need the process message reply to save
    // the outgoing IBC packet identifier for later.
    let submsg = msg_with_sudo_callback(deps.branch(), cosmos_msg, SudoPayload {
        port_id: get_port_id(env.contract.address.as_str(), &interchain_account_id),
        message: format!("user_withdraw_{}_{}_{}", info.sender, pool_addr, unstake_index_list_str),
        tx_type: TxType::UserWithdraw,
    })?;

    Ok(
        Response::new()
            .add_attribute("action", "withdraw")
            .add_attribute("from", info.sender)
            .add_attribute("pool", pool_addr.clone())
            .add_attribute("unstake_index_list", unstake_index_list_str)
            .add_attribute("amount", total_withdraw_amount)
            .add_submessage(submsg)
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

fn execute_add_pool_validators(
    deps: DepsMut<NeutronQuery>,
    pool_addr: String,
    validator_addrs: Vec<String>
) -> NeutronResult<Response<NeutronMsg>> {
    let pool_info = POOLS.load(deps.as_ref().storage, pool_addr.clone())?;

    let latest_delegation_query_id = LATEST_DELEGATIONS_QUERY_ID.load(deps.as_ref().storage)?;
    let pool_delegation_query_id = latest_delegation_query_id + 1;

    let register_delegation_query_msg = new_register_delegator_delegations_query_msg(
        pool_info.connection_id.clone(),
        pool_addr.clone(),
        validator_addrs,
        DEFAULT_UPDATE_PERIOD
    )?;

    // wrap into submessage to save {query_id, query_type} on reply that'll later be used to handle sudo kv callback
    let register_delegation_query_submsg = SubMsg::reply_on_success(
        register_delegation_query_msg,
        pool_delegation_query_id
    );

    LATEST_DELEGATIONS_QUERY_ID.save(deps.storage, &(latest_delegation_query_id + 1))?;

    // todo update pool_info in query replay
    Ok(Response::default().add_submessage(register_delegation_query_submsg))
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

    let target_validator = match find_redelegation_target(&delegations, &validator_addrs) {
        Some(target_validator) => target_validator,
        None => {
            return Err(NeutronError::Std(StdError::generic_err("find_redelegation_target failed")));
        }
    };

    let mut msgs = vec![];

    for src_validator in validator_addrs {
        let amount = match find_validator_amount(&delegations, src_validator.clone()) {
            Some(amount) => amount,
            None => {
                continue;
            }
        };
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
        let submsg_redelegate = msg_with_sudo_callback(deps.branch(), cosmos_msg, SudoPayload {
            port_id: get_port_id(env.contract.address.to_string(), interchain_account_id.clone()),
            message: "interchain_undelegate".to_string(),
            tx_type: TxType::RmValidator,
        })?;
        msgs.push(submsg_redelegate);
    }

    // todo: update state in sudo reply
    // todo: update delegation_query in sudo reply
    // todo: update pool validator list
    Ok(Response::default().add_submessages(msgs))
}

fn execute_era_update(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    channel: String,
    pool_addr: String
) -> NeutronResult<Response<NeutronMsg>> {
    let unstaks = UNSTAKES_OF_INDEX.range(deps.storage, None, None, Order::Ascending);
    let mut need_withdraw = Uint128::zero();
    for unstake in unstaks {
        let (_, unstake_info) = unstake?;
        need_withdraw = need_withdraw.add(unstake_info.amount);
    }

    // --------------------------------------------------------------------------------------------------
    // contract must pay for relaying of acknowledgements
    // See more info here: https://docs.neutron.org/neutron/feerefunder/overview
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let mut msgs = vec![];
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    // check era state
    if pool_info.era_update_status != ActiveReported {
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

    let interchain_account_id = POOL_ICA_MAP.load(deps.storage, pool_addr.clone())?;

    let submsg_pool_ibc_send = msg_with_sudo_callback(deps.branch(), msg, SudoPayload {
        port_id: get_port_id(env.contract.address.to_string(), interchain_account_id.clone()),
        message: "era_update_ibc_token_send".to_string(),
        tx_type: TxType::EraUpdateIbcSend,
    })?;
    deps.as_ref().api.debug(
        format!("WASMDEBUG: execute_send: sent submsg: {:?}", submsg_pool_ibc_send).as_str()
    );
    msgs.push(submsg_pool_ibc_send);

    // check withdraw address balance and send it to the pool
    let withdraw_balances: Balances = query_balance_by_addr(
        deps.as_ref(),
        pool_info.withdraw_addr.clone()
    )?.balances;

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

    pool_info.era_update_status = EraUpdated;
    pool_info.need_withdraw = need_withdraw;
    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;
    if withdraw_amount == 0 {
        return Ok(Response::default());
    }

    // todo: Check whether the delegator-validator needs to manually withdraw
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
        SudoPayload {
            port_id: get_port_id(env.contract.address.to_string(), interchain_account_id),
            message: "era_update_withdraw_token_send".to_string(),
            tx_type: TxType::EraUpdateWithdrawSend,
        }
    )?;
    deps.as_ref().api.debug(
        format!("WASMDEBUG: execute_send: sent submsg: {:?}", submsg_withdraw_ibc_send).as_str()
    );
    msgs.push(submsg_withdraw_ibc_send);

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
    if pool_info.era_update_status != EraUpdated {
        deps.as_ref().api.debug(
            format!("WASMDEBUG: execute_era_bond skip pool: {:?}", pool_addr).as_str()
        );
        return Ok(Response::new());
    }

    let interchain_account_id = POOL_ICA_MAP.load(deps.storage, pool_addr.clone())?;
    if pool_info.unbond > pool_info.active {
        let unbond_amount = pool_info.unbond - pool_info.active;

        let delegations = query_delegation_by_addr(deps.as_ref(), pool_addr.clone())?;

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
            let submsg_unstake = msg_with_sudo_callback(deps.branch(), cosmos_msg, SudoPayload {
                port_id: get_port_id(
                    env.contract.address.to_string(),
                    interchain_account_id.clone()
                ),
                // Here you can store some information about the transaction to help you parse
                // the acknowledgement later.
                message: "interchain_undelegate".to_string(),
                tx_type: TxType::EraBondUnstake,
            })?;

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
            let submsg_stake = msg_with_sudo_callback(deps.branch(), cosmos_msg, SudoPayload {
                port_id: get_port_id(
                    env.contract.address.to_string(),
                    interchain_account_id.clone()
                ),
                // Here you can store some information about the transaction to help you parse
                // the acknowledgement later.
                message: "interchain_delegate".to_string(),
                tx_type: TxType::EraBondStake,
            })?;
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

    let delegations = query_delegation_by_addr(deps.as_ref(), pool_addr.clone())?;

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
        QUERY_BALANCES_REPLY_ID_RANGE_START..=QUERY_BALANCES_REPLY_ID_END =>
            write_balance_query_id_to_reply_id(deps, msg),
        QUERY_DELEGATIONS_REPLY_ID_RANGE_START..=QUERY_DELEGATIONS_REPLY_ID_END =>
            write_delegation_query_id_to_reply_id(deps, msg),
        _ => Err(StdError::generic_err(format!("unsupported reply message id {}", msg.id))),
    }
}

// save query_id to query_type information in reply, so that we can understand the kind of query we're getting in sudo kv call
fn write_balance_query_id_to_reply_id(deps: DepsMut, reply: Reply) -> StdResult<Response> {
    let resp: MsgRegisterInterchainQueryResponse = serde_json_wasm
        ::from_slice(
            reply.result
                .into_result()
                .map_err(StdError::generic_err)?
                .data.ok_or_else(|| StdError::generic_err("no result"))?
                .as_slice()
        )
        .map_err(|e| StdError::generic_err(format!("failed to parse response: {:?}", e)))?;

    deps.api.debug(
        format!("WASMDEBUG: write_balance_query_id_to_reply_id query_id: {:?}", resp.id).as_str()
    );

    // then in success reply handler we do thiss
    KV_QUERY_ID_TO_CALLBACKS.save(deps.storage, resp.id, &QueryKind::Balances)?;
    OWN_QUERY_ID_TO_ICQ_ID.save(deps.storage, reply.id, &resp.id)?;

    Ok(Response::default())
}

fn write_delegation_query_id_to_reply_id(deps: DepsMut, reply: Reply) -> StdResult<Response> {
    let resp: MsgRegisterInterchainQueryResponse = serde_json_wasm
        ::from_slice(
            reply.result
                .into_result()
                .map_err(StdError::generic_err)?
                .data.ok_or_else(|| StdError::generic_err("no result"))?
                .as_slice()
        )
        .map_err(|e| StdError::generic_err(format!("failed to parse query response: {:?}", e)))?;

    // then in success reply handler we do thiss
    KV_QUERY_ID_TO_CALLBACKS.save(deps.storage, resp.id, &QueryKind::Delegations)?;
    OWN_QUERY_ID_TO_ICQ_ID.save(deps.storage, reply.id, &resp.id)?;

    deps.api.debug(
        format!("WASMDEBUG: write_delegation_query_id_to_reply_id query_id: {:?}", resp.id).as_str()
    );

    Ok(Response::default())
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

        _ => Ok(Response::default()),
    }
}

fn sudo_callback(deps: DepsMut, payload: SudoPayload) -> StdResult<Response> {
    match payload.tx_type {
        TxType::SetWithdrawAddr => {
            let parts: Vec<&str> = payload.message.split('_').collect();

            let delegator = parts.first().unwrap_or(&"").to_string();
            let withdraw_addr = parts.get(1).unwrap_or(&"").to_string();
            let mut pool_info = POOLS.load(deps.storage, delegator.clone())?;
            pool_info.withdraw_addr = withdraw_addr;
            POOLS.save(deps.storage, delegator, &pool_info)?;
        }

        _ => {}
    }
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

    if let Ok(payload) = read_sudo_payload(deps.storage, channel_id, seq_id) {
        return sudo_callback(deps, payload);
    }

    Err(StdError::generic_err("Error message"))
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
            parsed_version.address.clone(),
            &parsed_version.controller_connection_id
        )?;
        let pool_info = PoolInfo {
            need_withdraw: Uint128::zero(),
            unbond: Uint128::zero(),
            active: Uint128::zero(),
            rtoken: Addr::unchecked(""),
            withdraw_addr: "".to_string(),
            pool_addr: parsed_version.address.clone(),
            ibc_denom: "".to_string(),
            remote_denom: "".to_string(),
            connection_id: "".to_string(),
            validator_addrs: vec![],
            era: 0,
            rate: Uint128::zero(),
            minimal_stake: Uint128::zero(),
            unstake_times_limit: Uint128::zero(),
            next_unstake_index: Uint128::zero(),
            unbonding_period: 0,
            era_update_status: PoolBondState::ActiveReported,
            unbond_commission: Uint128::zero(),
            protocol_fee_receiver: Addr::unchecked(""),
        };
        POOLS.save(deps.storage, parsed_version.address.clone(), &pool_info)?;
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

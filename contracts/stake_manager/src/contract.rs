use std::vec;

use cosmwasm_std::{
    entry_point,
    from_json,
    to_json_binary,
    Binary,
    CosmosMsg,
    CustomQuery,
    Deps,
    DepsMut,
    Env,
    MessageInfo,
    Reply,
    Response,
    StdError,
    StdResult,
    SubMsg,
};
use cw2::set_contract_version;
use neutron_sdk::interchain_txs::helpers::get_port_id;
use neutron_sdk::sudo::msg::SudoMsg;
use neutron_sdk::{
    bindings::{ msg::{ IbcFee, MsgIbcTransferResponse, NeutronMsg }, query::NeutronQuery },
    interchain_queries::get_registered_query,
    sudo::msg::RequestPacket,
    NeutronResult,
};
use schemars::JsonSchema;
use serde::{ Deserialize, Serialize };

use crate::execute_config_pool::{execute_config_pool, sudo_config_pool_callback};
use crate::execute_era_bond::execute_era_bond;
use crate::execute_era_bond_active::execute_bond_active;
use crate::execute_era_update::execute_era_update;
use crate::execute_pool_add_validators::execute_add_pool_validators;
use crate::execute_pool_rm_validators::execute_rm_pool_validators;
use crate::execute_register_pool::{ execute_register_pool, sudo_open_ack };
use crate::execute_register_query::{ register_balance_query, register_delegations_query };
use crate::execute_stake::execute_stake;
use crate::execute_stake_lsm::execute_stake_lsm;
use crate::execute_unstake::execute_unstake;
use crate::execute_withdraw::{ execute_withdraw, sudo_withdraw_callback };
use crate::query::{
    query_acknowledgement_result,
    query_balance,
    query_errors_queue,
    query_interchain_address,
    query_interchain_address_contract,
    query_pool_info,
    query_user_unstake,
};
use crate::query_callback::{
    write_balance_query_id_to_reply_id,
    write_delegation_query_id_to_reply_id,
};
use crate::state::{
    State,
    IBC_SUDO_ID_RANGE_END,
    IBC_SUDO_ID_RANGE_START,
    INTERCHAIN_ACCOUNTS,
    QUERY_BALANCES_REPLY_ID_END,
    QUERY_DELEGATIONS_REPLY_ID_END,
    STATE,
};
use crate::{
    msg::{ ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg },
    state::{
        read_reply_payload,
        read_sudo_payload,
        save_reply_payload,
        save_sudo_payload,
        LATEST_BALANCES_QUERY_ID,
        LATEST_DELEGATIONS_QUERY_ID,
        QUERY_BALANCES_REPLY_ID_RANGE_START,
        QUERY_DELEGATIONS_REPLY_ID_RANGE_START,
    },
};

// Default timeout for IbcTransfer is 10000000 blocks
pub const DEFAULT_TIMEOUT_HEIGHT: u64 = 10000000;

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

    STATE.save(deps.storage, &(State { owner: info.sender }))?;

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
        ExecuteMsg::ConfigPool(params) => execute_config_pool(deps, env, info, *params),
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
        ExecuteMsg::EraUpdate {
            channel,
            pool_addr,
        } => { // Different rtoken are executed separately.
            execute_era_update(deps, env, channel, pool_addr)
        }
        ExecuteMsg::EraBond { pool_addr } => execute_era_bond(deps, env, pool_addr),
        ExecuteMsg::EraBondActive { pool_addr } => execute_bond_active(deps, env, pool_addr),
        ExecuteMsg::StakeLSM {} => execute_stake_lsm(deps, env, info),
    }
}

#[entry_point]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> StdResult<Response> {
    deps.api.debug(format!("WASMDEBUG: reply msg: {:?}", msg).as_str());
    match msg.id {
        // It's convenient to use range of ID's to handle multiple reply messages
        IBC_SUDO_ID_RANGE_START..=IBC_SUDO_ID_RANGE_END => prepare_sudo_payload(deps, env, msg),
        QUERY_BALANCES_REPLY_ID_RANGE_START..=QUERY_BALANCES_REPLY_ID_END => {
            write_balance_query_id_to_reply_id(deps, msg)
        }
        QUERY_DELEGATIONS_REPLY_ID_RANGE_START..=QUERY_DELEGATIONS_REPLY_ID_END => {
            write_delegation_query_id_to_reply_id(deps, msg)
        }
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

        _ => Ok(Response::default()),
    }
}

fn sudo_callback(deps: DepsMut, payload: SudoPayload) -> StdResult<Response> {
    match payload.tx_type {
        TxType::SetWithdrawAddr => sudo_config_pool_callback(deps, payload),
        TxType::UserWithdraw => sudo_withdraw_callback(deps, payload),

        _ => Ok(Response::new()),
    }
}

// saves payload to process later to the storage and returns a SubmitTX Cosmos SubMsg with necessary reply id
pub fn msg_with_sudo_callback<C: Into<CosmosMsg<T>>, T>(
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

pub fn min_ntrn_ibc_fee(fee: IbcFee) -> IbcFee {
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

pub fn get_ica(
    deps: Deps<impl CustomQuery>,
    env: &Env,
    interchain_account_id: &str
) -> Result<(String, String), StdError> {
    let key = get_port_id(env.contract.address.as_str(), interchain_account_id);

    INTERCHAIN_ACCOUNTS.load(deps.storage, key)?.ok_or_else(||
        StdError::generic_err("Interchain account is not created yet")
    )
}

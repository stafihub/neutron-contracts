use cosmwasm_std::{Addr, Binary, from_json, Order, StdResult, Storage, to_json_vec, Uint128};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::contract::{SudoPayload, TxType};

pub const IBC_SUDO_ID_RANGE_START: u64 = 1_000_000_000;
pub const IBC_SUDO_ID_RANGE_SIZE: u64 = 1_000;
pub const IBC_SUDO_ID_RANGE_END: u64 = IBC_SUDO_ID_RANGE_START + IBC_SUDO_ID_RANGE_SIZE;

pub const QUERY_BALANCES_REPLY_ID_RANGE_START: u64 = 1000;
pub const QUERY_BALANCES_REPLY_ID_RANGE_SIZE: u64 = 500;
pub const QUERY_BALANCES_REPLY_ID_END: u64 =
    QUERY_BALANCES_REPLY_ID_RANGE_START + QUERY_BALANCES_REPLY_ID_RANGE_SIZE;

pub const QUERY_DELEGATIONS_REPLY_ID_RANGE_START: u64 = 2000;
pub const QUERY_DELEGATIONS_REPLY_ID_RANGE_SIZE: u64 = 500;
pub const QUERY_DELEGATIONS_REPLY_ID_END: u64 =
    QUERY_DELEGATIONS_REPLY_ID_RANGE_START + QUERY_DELEGATIONS_REPLY_ID_RANGE_SIZE;

pub const REPLY_QUEUE_ID: Map<u64, Vec<u8>> = Map::new("reply_queue_id");

const REPLY_ID: Item<u64> = Item::new("reply_id");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct State {
    pub owner: Addr,
}

pub const STATE: Item<State> = Item::new("state");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PoolValidatorStatus{
    pub status: ValidatorUpdateStatus
}

pub const POOL_VALIDATOR_STATUS: Map<String, PoolValidatorStatus> = Map::new("pool_validator_status");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PoolInfo {
    pub bond: Uint128,
    pub unbond: Uint128,
    pub active: Uint128,
    pub rtoken: Addr,
    pub withdraw_addr: String,
    pub pool_addr: String,
    pub ibc_denom: String,
    pub remote_denom: String,
    pub connection_id: String,
    pub validator_addrs: Vec<String>,
    pub era: u64,
    pub rate: Uint128,
    pub era_seconds: u64,
    pub offset: u64,
    pub minimal_stake: Uint128,
    pub unstake_times_limit: u64,
    pub next_unstake_index: u64,
    pub unbonding_period: u64,
    pub era_update_status: PoolBondState,
    pub unbond_commission: Uint128,
    pub protocol_fee_receiver: Addr,
}

pub const POOLS: Map<String, PoolInfo> = Map::new("pools");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct EraShot {
    pub pool_addr: String,
    pub era: u64,
    pub bond: Uint128,
    pub unbond: Uint128,
    pub active: Uint128,
    pub bond_height: u64,
    pub failed_tx: Option<TxType>,
}

pub const POOL_ERA_SHOT: Map<String, EraShot> = Map::new("pool_era_shot");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PoolBondState {
    EraUpdated,
    BondReported,
    WithdrawReported,
    ActiveReported,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ValidatorUpdateStatus {
    Pending,
    Success,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum WithdrawStatus {
    Default,
    Pending,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UnstakeInfo {
    pub era: u64,
    pub index: String,
    pub pool_addr: String,
    pub amount: Uint128,
    pub status: WithdrawStatus,
}

pub const UNSTAKES_OF_INDEX: Map<String, UnstakeInfo> = Map::new("unstakes_of_index");

// Option<(String, String) 1-pool_addr 2-String-> index -> pool_addr+"-"+will_unstake_index
pub const UNSTAKES_INDEX_FOR_USER: Map<&Addr, Vec<Option<(String, String)>>> =
    Map::new("unstakes_index_for_user");

//  key: port_id value: Option<pool_addr,interchain_connection_id>
pub const INTERCHAIN_ACCOUNTS: Map<String, Option<(String, String)>> =
    Map::new("interchain_accounts");

//  key: pool_addr value: interchain_account_id
pub const ADDR_ICAID_MAP: Map<String, String> = Map::new("pool_ica_map");

// key: connection_id value:pool_addr list
pub const CONNECTION_POOL_MAP: Map<String, Vec<String>> = Map::new("connection_pools");

// key: ica address value: query id
pub const ADDR_BALANCES_QUERY_ID: Map<String, u64> = Map::new("addr_balances_query_id");

pub const ADDR_DELEGATIONS_QUERY_ID: Map<String, u64> = Map::new("addr_delegations_query_id");

pub const LATEST_BALANCES_QUERY_ID: Item<u64> = Item::new("latest_balances_query_id");

pub const LATEST_DELEGATIONS_QUERY_ID: Item<u64> = Item::new("latest_delegations_query_id");

pub const KV_QUERY_ID_TO_CALLBACKS: Map<u64, QueryKind> = Map::new("kv_query_id_to_callbacks");

pub const OWN_QUERY_ID_TO_ICQ_ID: Map<u64, u64> = Map::new("own_query_id_to_icq_id");

// contains query kinds that we expect to handle in `sudo_kv_query_result`
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub enum QueryKind {
    // Balance query
    Balances,
    Delegations,
    // You can add your handlers to understand what query to deserialize by query_id in sudo callback
}

/// Serves for storing acknowledgement calls for interchain transactions
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AcknowledgementResult {
    /// Success - Got success acknowledgement in sudo with array of message item types in it
    Success(Vec<String>),
    /// Error - Got error acknowledgement in sudo with payload message in it and error details
    Error((String, String)),
    /// Timeout - Got timeout acknowledgement in sudo with payload message in it
    Timeout(String),
}

// interchain transaction responses - ack/err/timeout state to query later
pub const ACKNOWLEDGEMENT_RESULTS: Map<(String, u64), AcknowledgementResult> =
    Map::new("acknowledgement_results");

pub const ERRORS_QUEUE: Map<u32, String> = Map::new("errors_queue");

pub fn read_errors_from_queue(store: &dyn Storage) -> StdResult<Vec<(Vec<u8>, String)>> {
    ERRORS_QUEUE
        .range_raw(store, None, None, Order::Ascending)
        .collect()
}

/// get_next_id gives us an id for a reply msg
/// dynamic reply id helps us to pass sudo payload to sudo handler via reply handler
/// by setting unique(in transaction lifetime) id to the reply and mapping our payload to the id
/// execute ->(unique reply.id) reply (channel_id,seq_id)-> sudo handler
/// Since id uniqueless id only matters inside a transaction,
/// we can safely reuse the same id set in every new transaction
pub fn get_next_id(store: &mut dyn Storage) -> StdResult<u64> {
    let mut id = REPLY_ID.may_load(store)?.unwrap_or(IBC_SUDO_ID_RANGE_START);
    if id > IBC_SUDO_ID_RANGE_END {
        id = IBC_SUDO_ID_RANGE_START;
    }
    REPLY_ID.save(store, &(id + 1))?;
    Ok(id)
}

pub fn save_reply_payload(store: &mut dyn Storage, payload: SudoPayload) -> StdResult<u64> {
    let id = get_next_id(store)?;
    REPLY_QUEUE_ID.save(store, id, &to_json_vec(&payload)?)?;
    Ok(id)
}

pub fn read_reply_payload(store: &dyn Storage, id: u64) -> StdResult<SudoPayload> {
    let data = REPLY_QUEUE_ID.load(store, id)?;
    from_json(Binary(data))
}

/// SUDO_PAYLOAD - tmp storage for sudo handler payloads
/// key (String, u64) - (channel_id, seq_id)
/// every ibc chanel have its own sequence counter(autoincrement)
/// we can catch the counter in the reply msg for outgoing sudo msg
/// and save our payload for the msg
pub const SUDO_PAYLOAD: Map<(String, u64), Vec<u8>> = Map::new("sudo_payload");

pub fn save_sudo_payload(
    store: &mut dyn Storage,
    channel_id: String,
    seq_id: u64,
    payload: SudoPayload,
) -> StdResult<()> {
    SUDO_PAYLOAD.save(store, (channel_id, seq_id), &to_json_vec(&payload)?)
}

pub fn read_sudo_payload(
    store: &dyn Storage,
    channel_id: String,
    seq_id: u64,
) -> StdResult<SudoPayload> {
    let data = SUDO_PAYLOAD.load(store, (channel_id, seq_id))?;
    from_json(Binary(data))
}

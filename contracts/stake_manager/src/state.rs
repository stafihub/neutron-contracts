use cosmwasm_std::{
    from_json,
    Binary,
    StdResult,
    Storage,
    to_json_vec,
    Coin,
    Addr,
    Uint128,
    Int256,
};
use cw_storage_plus::{ Item, Map };
use schemars::JsonSchema;
use serde::{ Deserialize, Serialize };

use crate::contract::SudoPayload;

pub const IBC_SUDO_ID_RANGE_START: u64 = 1_000_000_000;
pub const IBC_SUDO_ID_RANGE_SIZE: u64 = 1_000;
pub const IBC_SUDO_ID_RANGE_END: u64 = IBC_SUDO_ID_RANGE_START + IBC_SUDO_ID_RANGE_SIZE;

pub const REPLY_QUEUE_ID: Map<u64, Vec<u8>> = Map::new("reply_queue_id");

const REPLY_ID: Item<u64> = Item::new("reply_id");

// todo: Organize the use of Uint128 and u128
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct State {
    pub minimal_stake: Coin,
    pub owner: Addr,
    pub cw20: Addr,
    pub unstake_times_limit: Uint128,
    pub next_unstake_index: Uint128,
    pub unbonding_period: u128,
}

pub const STATE: Item<State> = Item::new("state");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PoolInfo {
    pub need_withdraw: Int256,
    pub unbond: Int256,
    pub active: Int256,
    pub withdraw_addr: String,
    pub pool_addr: String,
    pub ibc_denom: String,
    pub connection_id: String,
    pub validator_addrs: Vec<String>,
}

pub const POOLS: Map<String, PoolInfo> = Map::new("pools");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PoolBondState {
    EraUpdated = 0,
    BondReported = 1,
    ActiveReported = 2,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Era {
    pub era: u128,
    pub pre_era: u128,
    pub rate: Uint128,
    pub pre_rate: Uint128,
    pub era_update_status: PoolBondState,
}
// key: pool_addr
pub const POOL_ERA_INFO: Map<String, Era> = Map::new("pool_era_info");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UnstakeInfo {
    pub era: u128,
    pub pool: String,
    pub amount: Uint128,
}

pub const UNSTAKES_OF_INDEX: Map<u128, UnstakeInfo> = Map::new("unstakes_of_index");

pub const UNSTAKES_INDEX_FOR_USER: Map<&Addr, Vec<Uint128>> = Map::new("unstakes_index_for_user");

// key: pool_addr value: denom
pub const POOL_DENOM_MPA: Map<String, String> = Map::new("pool_denom_mpa");

//  key: port_id value: Option<pool_addr,interchain_connection_id>
pub const INTERCHAIN_ACCOUNTS: Map<String, Option<(String, String)>> = Map::new(
    "interchain_accounts"
);

//  key: pool_addr value: interchain_account_id
pub const POOL_ICA_MAP: Map<String, String> = Map::new("pool_ica_map");

// key: connection_id value:pool_addr list
pub const CONNECTION_POOL_MAP: Map<String, Vec<String>> = Map::new("connection_pools");

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
    from_json(&Binary(data))
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
    payload: SudoPayload
) -> StdResult<()> {
    SUDO_PAYLOAD.save(store, (channel_id, seq_id), &to_json_vec(&payload)?)
}

pub fn read_sudo_payload(
    store: &dyn Storage,
    channel_id: String,
    seq_id: u64
) -> StdResult<SudoPayload> {
    let data = SUDO_PAYLOAD.load(store, (channel_id, seq_id))?;
    from_json(&Binary(data))
}

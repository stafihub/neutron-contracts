use cosmwasm_schema::cw_serde;
use cosmwasm_std::{from_json, to_json_vec, Addr, Binary, StdResult, Storage, Uint128};
use cw_storage_plus::{Item, Map};

use crate::helper::{
    QUERY_REPLY_ID_RANGE_END, QUERY_REPLY_ID_RANGE_START, REPLY_ID_RANGE_END, REPLY_ID_RANGE_START,
};

#[cw_serde]
pub struct Stack {
    pub admin: Addr,
    pub stack_fee_receiver: Addr,
    pub stack_fee_commission: Uint128,
    pub total_stack_fee: Uint128,
    pub pools: Vec<String>,
    pub lsd_token_code_id: u64,
}

pub const STACK: Item<Stack> = Item::new("stack");

#[cw_serde]
pub struct EraSnapshot {
    pub era: u64,
    pub bond: Uint128,
    pub unbond: Uint128,
    pub active: Uint128,
    pub restake_amount: Uint128,
    pub bond_height: u64,
}

#[cw_serde]
pub struct PoolInfo {
    pub bond: Uint128,
    pub unbond: Uint128,
    pub active: Uint128,
    pub lsd_token: Addr,
    pub ica_id: String,
    pub ibc_denom: String,
    pub channel_id_of_ibc_denom: String,
    pub remote_denom: String,
    pub validator_addrs: Vec<String>,
    pub era: u64,
    pub rate: Uint128,
    pub era_seconds: u64,
    pub offset: u64,
    pub minimal_stake: Uint128,
    pub unstake_times_limit: u64,
    pub next_unstake_index: u64,
    pub unbonding_period: u64,
    pub era_process_status: EraProcessStatus,
    pub validator_update_status: ValidatorUpdateStatus,
    pub unbond_commission: Uint128,
    pub platform_fee_commission: Uint128,
    pub total_platform_fee: Uint128,
    pub total_lsd_token_amount: Uint128,
    pub platform_fee_receiver: Addr,
    pub admin: Addr,
    pub share_tokens: Vec<cosmwasm_std::Coin>,
    pub redeemming_share_token_denom: Vec<String>,
    pub era_snapshot: EraSnapshot,
    pub paused: bool,
    pub lsm_support: bool,
    pub lsm_pending_limit: u64,
    pub rate_change_limit: Uint128,
}

pub const POOLS: Map<String, PoolInfo> = Map::new("pools");

#[cw_serde]
pub enum EraProcessStatus {
    EraUpdateStarted,
    EraUpdateEnded,
    BondStarted,
    BondEnded,
    WithdrawStarted,
    WithdrawEnded,
    RebondStarted,
    RebondEnded,
    ActiveEnded,
}

#[cw_serde]
pub enum ValidatorUpdateStatus {
    Start,
    WaitQueryUpdate,
    End,
}

#[cw_serde]
pub enum WithdrawStatus {
    Default,
    Pending,
}

#[cw_serde]
pub struct UnstakeInfo {
    pub era: u64,
    pub pool_addr: String,
    pub unstaker: String,
    pub amount: Uint128,
    pub status: WithdrawStatus,
    pub index: u64,
}

// (poolAddress,unstakeIndex)
pub const UNSTAKES_OF_INDEX: Map<(String, u64), UnstakeInfo> = Map::new("unstakes_of_index");

#[cw_serde]
pub struct IcaInfo {
    pub ctrl_connection_id: String,
    pub host_connection_id: String,
    pub ctrl_channel_id: String,
    pub host_channel_id: String,
    pub ctrl_port_id: String,
    pub ica_addr: String,
}

//  key: ica id value: (pool IcaInfo, withdraw icaInfo, admin)
pub const INFO_OF_ICA_ID: Map<String, (IcaInfo, IcaInfo, Addr)> = Map::new("info_of_ica_id");

// (userAddress,poolAddress) => []unstakeIndex
pub const UNSTAKES_INDEX_FOR_USER: Map<(Addr, String), Vec<u64>> =
    Map::new("unstakes_index_for_user");

// contains query kinds that we expect to handle in `sudo_kv_query_result`
#[cw_serde]
pub enum QueryKind {
    // Balance query
    Balances,
    Delegations,
    Validators,
    // You can add your handlers to understand what query to deserialize by query_id in sudo callback
}

impl QueryKind {
    pub fn to_string(self) -> String {
        match self {
            QueryKind::Balances => "balances".to_string(),
            QueryKind::Delegations => "delegations".to_string(),
            QueryKind::Validators => "validators".to_string(),
        }
    }
}

/// get_next_id gives us an id for a reply msg
/// dynamic reply id helps us to pass sudo payload to sudo handler via reply handler
/// by setting unique(in transaction lifetime) id to the reply and mapping our payload to the id
/// execute ->(unique reply.id) reply (channel_id,seq_id)-> sudo handler
/// Since id uniqueless id only matters inside a transaction,
/// we can safely reuse the same id set in every new transaction
const LATEST_REPLY_ID: Item<u64> = Item::new("latest_reply_id");
pub fn get_next_reply_id(store: &mut dyn Storage) -> StdResult<u64> {
    let id = LATEST_REPLY_ID
        .may_load(store)?
        .unwrap_or(REPLY_ID_RANGE_START);
    let mut save_id = id + 1;
    if save_id > REPLY_ID_RANGE_END {
        save_id = REPLY_ID_RANGE_START;
    }
    LATEST_REPLY_ID.save(store, &save_id)?;
    Ok(id)
}

const LATEST_QUERY_REPLY_ID: Item<u64> = Item::new("latest_query_reply_id");
pub fn get_next_query_reply_id(store: &mut dyn Storage) -> StdResult<u64> {
    let id = LATEST_QUERY_REPLY_ID
        .may_load(store)?
        .unwrap_or(QUERY_REPLY_ID_RANGE_START);
    let mut save_id = id + 1;
    if save_id > QUERY_REPLY_ID_RANGE_END {
        save_id = QUERY_REPLY_ID_RANGE_START;
    }
    LATEST_QUERY_REPLY_ID.save(store, &save_id)?;
    Ok(id)
}

#[cw_serde]
pub enum TxType {
    SetWithdrawAddr,
    UpdateValidator,
    RmValidator,
    AddValidator,
    UserWithdraw,
    EraUpdate,
    EraBond,
    EraCollectWithdraw,
    EraRebond,
    RedeemTokenForShare,
    StakeLsm,
}
#[cw_serde]
pub struct SudoPayload {
    pub message: String,
    pub pool_addr: String,
    pub port_id: String,
    pub tx_type: TxType,
}

pub const REPLY_ID_TO_PAYLOAD: Map<u64, Vec<u8>> = Map::new("reply_id_to_payload");
pub fn save_reply_payload(store: &mut dyn Storage, payload: SudoPayload) -> StdResult<u64> {
    let id = get_next_reply_id(store)?;
    REPLY_ID_TO_PAYLOAD.save(store, id, &to_json_vec(&payload)?)?;
    Ok(id)
}
pub fn read_reply_payload(store: &dyn Storage, id: u64) -> StdResult<SudoPayload> {
    let data = REPLY_ID_TO_PAYLOAD.load(store, id)?;
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

// key: (ica address, query kind) value: query reply id
pub const ADDRESS_TO_REPLY_ID: Map<(String, String), u64> =
    Map::new("address_querykind_to_reply_id");

pub const REPLY_ID_TO_QUERY_ID: Map<u64, u64> = Map::new("reply_id_to_query_id");

// reply id -> (true)
// just save in validators query init
pub const REPLY_ID_TO_NEED_UPDATE: Map<u64, bool> = Map::new("reply_id_to_need_update");
pub const QUERY_ID_TO_REPLY_ID: Map<u64, u64> = Map::new("query_id_to_reply_id");

// validator -> vec[timestamp]
pub const VALIDATORS_UNBONDS_TIME: Map<String, Vec<u64>> = Map::new("validators_unbonds_time");

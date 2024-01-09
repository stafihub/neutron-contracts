use cosmwasm_std::{Addr, Coin, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct InstantiateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetRegisteredQuery {
        query_id: u64,
    },
    Balance {
        ica_addr: String,
    },
    Delegations {
        pool_addr: String,
    },
    PoolInfo {
        pool_addr: String,
    },
    EraSnapshot {
        pool_addr: String,
    },
    /// this query goes to neutron and get stored ICA with a specific query
    InterchainAccountAddress {
        interchain_account_id: String,
        connection_id: String,
    },
    // this query returns ICA from contract store, which saved from acknowledgement
    InterchainAccountAddressFromContract {
        interchain_account_id: String,
    },
    // this query returns acknowledgement result after interchain transaction
    AcknowledgementResult {
        interchain_account_id: String,
        sequence_id: u64,
    },
    UserUnstake {
        pool_addr: String,
        user_neutron_addr: Addr,
    },
    UserUnstakeIndex {
        pool_addr: String,
        user_neutron_addr: Addr,
    },
    // this query returns non-critical errors list
    ErrorsQueue {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InitPoolParams {
    pub interchain_account_id: String,
    pub unbond: Uint128,
    pub bond: Uint128,
    pub active: Uint128,
    pub ibc_denom: String,
    pub channel_id_of_ibc_denom: String,
    pub remote_denom: String,
    pub validator_addrs: Vec<String>,
    pub era: u64,
    pub rate: Uint128,
    pub total_platform_fee: Uint128,
    pub platform_fee_receiver: String,
    pub share_tokens: Vec<Coin>,
    pub lsd_token_name: String,
    pub lsd_token_symbol: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ConfigStackParams {
    pub stack_fee_receiver: Option<Addr>,
    pub new_admin: Option<Addr>,
    pub stack_fee_commission: Option<Uint128>,
    pub total_stack_fee: Option<Uint128>,
    pub add_operator: Option<Addr>,
    pub rm_operator: Option<Addr>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ConfigPoolParams {
    pub pool_addr: String,
    pub platform_fee_receiver: Option<String>,
    pub minimal_stake: Option<Uint128>,
    pub unstake_times_limit: Option<u64>,
    pub next_unstake_index: Option<u64>,
    pub unbonding_period: Option<u64>,
    pub unbond_commission: Option<Uint128>,
    pub platform_fee_commission: Option<Uint128>,
    pub era_seconds: Option<u64>,
    pub offset: Option<u64>,
    pub paused: Option<bool>,
    pub lsm_support: Option<bool>,
    pub lsm_pending_limit: Option<u64>,
    pub rate_change_limit: Option<Uint128>,
    pub new_admin: Option<Addr>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    RegisterPool {
        connection_id: String,
        interchain_account_id: String,
        register_fee: Vec<Coin>,
    },
    InitPool(Box<InitPoolParams>),
    ConfigPool(Box<ConfigPoolParams>),
    ConfigStack(Box<ConfigStackParams>),
    OpenChannel {
        pool_addr: String,
        closed_channel_id: String,
        register_fee: Vec<Coin>,
    },
    RedeemTokenForShare {
        pool_addr: String,
        tokens: Vec<Coin>,
    },
    Stake {
        neutron_address: String,
        pool_addr: String,
    },
    Unstake {
        amount: Uint128,
        pool_addr: String,
    },
    Withdraw {
        pool_addr: String,
        receiver: Addr,
        unstake_index_list: Vec<u64>,
    },
    PoolRmValidators {
        pool_addr: String,
        validator_addr: String,
    },
    PoolAddValidators {
        pool_addr: String,
        validator_addrs: Vec<String>,
    },
    PoolUpdateValidator {
        pool_addr: String,
        old_validator: String,
        new_validator: String,
    },
    EraUpdate {
        pool_addr: String,
    },
    EraBond {
        pool_addr: String,
    },
    EraCollectWithdraw {
        pool_addr: String,
    },
    EraRestake {
        pool_addr: String,
    },
    EraActive {
        pool_addr: String,
    },
    UpdateLsdTokenCodeId {
        code_id: u64,
    },
    StakeLsm {
        neutron_address: String,
        pool_addr: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct MigrateMsg {}

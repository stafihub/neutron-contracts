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
    pub rtoken: String,
    pub protocol_fee_receiver: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ConfigPoolParams {
    pub pool_addr: String,
    pub protocol_fee_receiver: Option<String>,
    pub minimal_stake: Option<Uint128>,
    pub unstake_times_limit: Option<u64>,
    pub next_unstake_index: Option<u64>,
    pub unbonding_period: Option<u64>,
    pub unbond_commission: Option<Uint128>,
    pub protocol_fee_commission: Option<Uint128>,
    pub era_seconds: Option<u64>,
    pub offset: Option<u64>,
    pub paused: Option<bool>,
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
    OpenChannel {
        pool_addr: String,
        closed_channel_id: String,
        register_fee: Vec<Coin>,
    },
    RedeemTokenForShare {
        pool_addr: String,
        tokens: Vec<Coin>,
    },
    RegisterBalanceQuery {
        connection_id: String,
        update_period: u64,
        addr: String,
        denom: String,
    },
    RegisterDelegatorDelegationsQuery {
        delegator: String,
        validators: Vec<String>,
        connection_id: String,
        update_period: u64,
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
        validator_addrs: Vec<String>,
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
    StakeLsm {
        neutron_address: String,
        pool_addr: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct MigrateMsg {}

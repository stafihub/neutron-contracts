use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Coin, Uint128};
use neutron_sdk::{
    bindings::query::{QueryInterchainAccountAddressResponse, QueryRegisteredQueryResponse},
    interchain_queries::v045::queries::{BalanceResponse, DelegatorDelegationsResponse},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state::{EraSnapshot, IcaInfo, PoolInfo, Stack, UnstakeInfo};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct InstantiateMsg {
    pub lsd_token_code_id: u64,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(QueryRegisteredQueryResponse)]
    GetRegisteredQuery { query_id: u64 },
    #[returns(BalanceResponse)]
    Balance { ica_addr: String },
    #[returns(DelegatorDelegationsResponse)]
    Delegations { pool_addr: String },
    #[returns(PoolInfo)]
    PoolInfo { pool_addr: String },
    #[returns(Stack)]
    StackInfo {},
    #[returns(EraSnapshot)]
    EraSnapshot { pool_addr: String },
    /// this query goes to neutron and get stored ICA with a specific query
    #[returns(QueryInterchainAccountAddressResponse)]
    InterchainAccountAddress {
        interchain_account_id: String,
        connection_id: String,
    },
    // this query returns ICA from contract store, which saved from acknowledgement
    #[returns((IcaInfo, IcaInfo, Addr))]
    InterchainAccountAddressFromContract { interchain_account_id: String },
    // this query returns acknowledgement result after interchain transaction
    #[returns(u64)]
    AcknowledgementResult {
        interchain_account_id: String,
        sequence_id: u64,
    },
    #[returns([UnstakeInfo])]
    UserUnstake {
        pool_addr: String,
        user_neutron_addr: Addr,
    },
    #[returns([String])]
    UserUnstakeIndex {
        pool_addr: String,
        user_neutron_addr: Addr,
    },
    // this query returns non-critical errors list
    #[returns(Vec < (Vec < u8 >, String) >)]
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
    pub total_lsd_token_amount: Option<Uint128>,
    pub platform_fee_receiver: String,
    pub share_tokens: Vec<Coin>,
    pub lsd_code_id: Option<u64>,
    pub lsd_token_name: String,
    pub lsd_token_symbol: String,
    pub era_seconds: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ConfigStackParams {
    pub stack_fee_receiver: Option<Addr>,
    pub new_admin: Option<Addr>,
    pub stack_fee_commission: Option<Uint128>,
    pub total_stack_fee: Option<Uint128>,
    pub lsd_token_code_id: Option<u64>,
    pub add_entrusted_pool: Option<String>,
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
    PoolRmValidator {
        pool_addr: String,
        validator_addr: String,
    },
    PoolAddValidator {
        pool_addr: String,
        validator_addr: String,
    },
    PoolUpdateValidator {
        pool_addr: String,
        old_validator: String,
        new_validator: String,
    },
    PoolUpdateQuery {
        pool_addr: String,
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

use cosmwasm_std::{ Addr, Uint128 };
use schemars::JsonSchema;
use serde::{ Deserialize, Serialize };

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct InstantiateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetRegisteredQuery {
        query_id: u64,
    },
    Balance {
        query_id: u64,
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
    // this query returns non-critical errors list
    ErrorsQueue {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    RegisterPool {
        connection_id: String,
        interchain_account_id: String,
    },
    ConfigPool {
        interchain_account_id: String,
        validator_addrs: Vec<String>,
        withdraw_addr: String,
        rtoken: Addr,
        minimal_stake: Uint128,
        unstake_times_limit: Uint128,
        next_unstake_index: Uint128,
        unbonding_period: u128,
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
        interchain_account_id: String,
        pool_addr: String,
    },
    Withdraw {
        pool_addr: String,
        receiver: Addr,
        interchain_account_id: String,
    },
    PoolRmValidator {
        pool_addr: String,
        validator_addrs: Vec<String>,
    },
    EraUpdate {
        channel: String,
        pool_addr: String,
    },
    EraBond {
        pool_addr: String,
    },
    EraBondActive {
        pool_addr: String,
    },
    StakeLSM {
        // todo!
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct MigrateMsg {}

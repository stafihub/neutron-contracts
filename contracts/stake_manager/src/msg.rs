use cosmwasm_std::{ Addr, Uint128, Coin };
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
    PoolInfo {
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
    // this query returns non-critical errors list
    ErrorsQueue {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    RegisterPool {
        connection_id: String,
        interchain_account_id: String,
        register_fee: Vec<Coin>,
    },
    ConfigPool {
        interchain_account_id: String,
        need_withdraw: Uint128,
        unbond: Uint128,
        active: Uint128,
        rtoken: Addr,
        withdraw_addr: String,
        ibc_denom: String,
        remote_denom: String,
        validator_addrs: Vec<String>,
        era: u128,
        rate: Uint128,
        minimal_stake: Uint128,
        unstake_times_limit: Uint128,
        next_unstake_index: Uint128,
        unbonding_period: u128,
        unbond_commission: Uint128,
        protocol_fee_receiver: Addr,
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
        interchain_account_id: String,
    },
    PoolRmValidator {
        pool_addr: String,
        validator_addrs: Vec<String>,
    },
    PoolAddValidator {
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

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
pub struct ConfigPoolParams {
    pub interchain_account_id: String,
    pub need_withdraw: Uint128,
    pub unbond: Uint128,
    pub active: Uint128,
    pub rtoken: Addr,
    pub withdraw_addr: String,
    pub ibc_denom: String,
    pub remote_denom: String,
    pub validator_addrs: Vec<String>,
    pub era: u64,
    pub rate: Uint128,
    pub era_seconds: u64,
    pub offset: u64,
    pub minimal_stake: Uint128,
    pub unstake_times_limit: Uint128,
    pub next_unstake_index: Uint128,
    pub unbonding_period: u64,
    pub unbond_commission: Uint128,
    pub protocol_fee_receiver: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    RegisterPool {
        connection_id: String,
        interchain_account_id: String,
        register_fee: Vec<Coin>,
    },
    ConfigPool(Box<ConfigPoolParams>),
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
    EraCollectWithdraw {
        channel: String,
        pool_addr: String,
    },
    EraActive {
        pool_addr: String,
    },
    StakeLSM {
        // todo!
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct MigrateMsg {}

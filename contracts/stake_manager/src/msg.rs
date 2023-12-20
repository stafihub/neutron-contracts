use cosmwasm_std::{ Addr, Coin, Uint128 };
use schemars::JsonSchema;
use serde::{ Deserialize, Serialize };

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct InstantiateMsg {
    pub minimal_stake: Coin,
    pub cw20_address: Addr,
    pub atom_ibc_denom: String,
    pub era: u128,
    pub rate: Uint128,
    pub cosmos_validator: String,
    pub unstake_times_limit: Uint128,
    pub next_unstake_index: Uint128,
    pub unbonding_period: u128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    RegisterICA {
        connection_id: String,
        interchain_account_id: String,
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
    NewEra {
        channel: String,
    },
    StakeLSM {
        // todo!
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct MigrateMsg {}

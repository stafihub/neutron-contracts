use cosmwasm_std::{Addr, Coin, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct InstantiateMsg {
	pub minimal_stake: Coin,
	pub connection_id: String,
	pub interchain_account_id: String,
	pub cw20_address: Addr,
	pub atom_ibc_denom: String,
	pub unstake_times_limit: Uint128,
	pub next_unstake_index: Uint128,
	pub unbonding_period: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
	Stake {
		neutron_address: String,
	},
	Unstake {
		amount: Uint128,
		interchain_account_id: String,
		receiver: Addr,
	},
	Withdraw {
		stake_pool: String,
	},
	NewEra {
		channel: String,
		interchain_account_id: String,
	},
	StakeLSM {
		// todo!
	}
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct MigrateMsg {}

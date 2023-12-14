use cosmwasm_std::{Addr, Coin};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct InstantiateMsg {
	pub minimal_stake: Coin,
	pub connection_id: String,
	pub interchain_account_id: String,
	pub cw20_address: Addr,
	pub atom_ibc_denom: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
	Stake {
		neutron_address: String,
	},
	Unstake {
		amount: u128,
	},
	Withdraw {},
	NewEra {
		channel: String,
		interchain_account_id: String,
	},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct MigrateMsg {}

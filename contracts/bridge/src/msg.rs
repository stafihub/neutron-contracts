use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;
use cosmwasm_std::Uint128;

use crate::state::BridgeInfo;
use crate::state::Proposal;

#[cw_serde]
pub struct InstantiateMsg {
    pub admin: Addr,
    pub threshold: u64,
    pub relayers: Vec<Addr>,
}

#[cw_serde]
pub enum ExecuteMsg {
    VoteProposal {
        chain_id: u64,      // source chain id
        deposit_nonce: u64, // deposit nonce from source chain
        resource_id: String,
        recipient: Addr,
        amount: Uint128,
    },
    AddRelayer {
        relayer: Addr,
    },
    RemoveRelayer {
        relayer: Addr,
    },
    AddResourceIdToToken {
        resource_id: String,
        token: Addr,
    },
    RemoveResourceId {
        resource_id: String,
    },
    ChangeThreshold {
        threshold: u64,
    },
    TransferAdmin {
        new_admin: String,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(BridgeInfo)]
    BridgeInfo {},
    #[returns(Proposal)]
    Proposal {
        chain_id: u64,      // source chain id
        deposit_nonce: u64, // deposit nonce from source chain
        resource_id: String,
        recipient: Addr,
        amount: Uint128,
    },
}

#[cw_serde]
pub struct MigrateMsg {}

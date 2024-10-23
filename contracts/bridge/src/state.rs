use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use sha2::{
    digest::{Digest, Update},
    Sha256,
};

#[cw_serde]
pub struct BridgeInfo {
    pub admin: Addr,
    pub threshold: u64,
    pub relayers: Vec<Addr>,
}

pub const BRIDGE_INFO: Item<BridgeInfo> = Item::new("bridge_info");

#[cw_serde]
pub struct Proposal {
    pub chain_id: u64,
    pub deposit_nonce: u64,
    pub resource_id: String,
    pub recipient: Addr,
    pub amount: Uint128,
    pub executed: bool,
    pub voters: Vec<Addr>,
}

pub fn get_proposal_id(
    chain_id: u64,
    deposit_nonce: u64,
    resource_id: &String,
    recipient: Addr,
    amount: Uint128,
) -> Vec<u8> {
    let mut key = Vec::<u8>::new();
    key.extend_from_slice(&chain_id.to_be_bytes());
    key.extend_from_slice(&deposit_nonce.to_be_bytes());
    key.extend_from_slice(resource_id.as_bytes());
    key.extend_from_slice(&recipient.as_bytes());
    key.extend_from_slice(&amount.to_be_bytes());

    return hash("proposalId", &key);
}

// hash(chain_id, deposit_nonce, resource_id, recipient, amount) => proposal
pub const PROPOSALS: Map<Vec<u8>, Proposal> = Map::new("proposals");

fn hash(ty: &str, key: &[u8]) -> Vec<u8> {
    let inner = Sha256::digest(ty.as_bytes());
    Sha256::new().chain(inner).chain(key).finalize().to_vec()
}

pub const RESOURCE_ID_TO_TOKEN: Map<String, Addr> = Map::new("resource_id_to_token");

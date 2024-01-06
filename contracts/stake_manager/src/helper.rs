use cosmos_sdk_proto::cosmos::base::v1beta1::Coin;
use cosmos_sdk_proto::cosmos::staking::v1beta1::{MsgBeginRedelegate, MsgDelegate};
use cosmos_sdk_proto::prost::Message;
use cosmwasm_std::{Binary, Deps, QueryRequest, StdResult, Uint128};

use neutron_sdk::bindings::msg::IbcFee;
use neutron_sdk::bindings::query::NeutronQuery;
use neutron_sdk::bindings::types::{KVKey, ProtobufAny};
use neutron_sdk::interchain_queries::helpers::decode_and_convert;
use neutron_sdk::interchain_queries::v045::helpers::{
    create_delegation_key, create_params_store_key, create_validator_key,
};
use neutron_sdk::interchain_queries::v045::types::{
    KEY_BOND_DENOM, PARAMS_STORE_KEY, STAKING_STORE_KEY,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const FEE_DENOM: &str = "untrn";
pub const ICA_WITHDRAW_SUFIX: &str = "-withdraw_addr";
pub const INTERCHAIN_ACCOUNT_ID_LEN_LIMIT: usize = 10;

pub fn min_ntrn_ibc_fee(fee: IbcFee) -> IbcFee {
    IbcFee {
        recv_fee: fee.recv_fee,
        ack_fee: fee
            .ack_fee
            .into_iter()
            .filter(|a| a.denom == FEE_DENOM)
            .collect(),
        timeout_fee: fee
            .timeout_fee
            .into_iter()
            .filter(|a| a.denom == FEE_DENOM)
            .collect(),
    }
}

pub fn gen_delegation_txs(
    delegator: String,
    validator: String,
    remote_denom: String,
    amount_for_this_validator: Uint128,
) -> ProtobufAny {
    // add sub message to stake
    let delegate_msg = MsgDelegate {
        delegator_address: delegator,
        validator_address: validator,
        amount: Some(Coin {
            denom: remote_denom,
            amount: amount_for_this_validator.to_string(),
        }),
    };

    // Serialize the Delegate message.
    let mut buf = Vec::new();
    buf.reserve(delegate_msg.encoded_len());

    let _ = delegate_msg.encode(&mut buf);

    // Put the serialized Delegate message to a types.Any protobuf message.
    ProtobufAny {
        type_url: "/cosmos.staking.v1beta1.MsgDelegate".to_string(),
        value: Binary::from(buf),
    }
}

pub fn gen_redelegate_txs(
    delegator: String,
    src_validator: String,
    target_validator: String,
    remote_denom: String,
    amount_for_this_validator: Uint128,
) -> ProtobufAny {
    let redelegate_msg = MsgBeginRedelegate {
        delegator_address: delegator.clone(),
        validator_src_address: src_validator.clone(),
        validator_dst_address: target_validator.clone(),
        amount: Some(Coin {
            denom: remote_denom.clone(),
            amount: amount_for_this_validator.to_string(),
        }),
    };

    // Serialize the Delegate message.
    let mut buf = Vec::new();
    buf.reserve(redelegate_msg.encoded_len());

    let _ = redelegate_msg.encode(&mut buf);

    // Put the serialized Delegate message to a types.Any protobuf message.
    ProtobufAny {
        type_url: "/cosmos.staking.v1beta1.BeginRedelegate".to_string(),
        value: Binary::from(buf),
    }
}

pub fn new_register_delegator_delegations_keys(
    delegator: String,
    validators: Vec<String>,
) -> Option<Vec<KVKey>> {
    let delegator_addr = decode_and_convert(delegator.as_str()).ok()?;

    // Allocate memory for such KV keys as:
    // * staking module params to get staking denomination
    // * validators structures to calculate amount of delegated tokens
    // * delegations structures to get info about delegations itself
    let mut keys: Vec<KVKey> = Vec::with_capacity(validators.len() * 2 + 1);

    // create KV key to get BondDenom from staking module params
    keys.push(KVKey {
        path: PARAMS_STORE_KEY.to_string(),
        key: Binary(create_params_store_key(STAKING_STORE_KEY, KEY_BOND_DENOM)),
    });

    for v in &validators {
        // create delegation key to get delegation structure
        let val_addr = decode_and_convert(v.as_str()).ok()?;
        keys.push(KVKey {
            path: STAKING_STORE_KEY.to_string(),
            key: Binary(create_delegation_key(&delegator_addr, &val_addr).ok()?),
        });

        // create validator key to get validator structure
        keys.push(KVKey {
            path: STAKING_STORE_KEY.to_string(),
            key: Binary(create_validator_key(&val_addr).ok()?),
        })
    }

    Some(keys)
}

pub fn get_withdraw_ica_id(interchain_account_id: String) -> String {
    format!("{}{}", interchain_account_id.clone(), ICA_WITHDRAW_SUFIX)
}

#[derive(Clone, PartialEq, Message)]
pub struct RawCoin {
    #[prost(string, tag = "1")]
    pub denom: String,
    #[prost(string, tag = "2")]
    pub amount: String,
}

impl From<cosmwasm_std::Coin> for RawCoin {
    fn from(value: cosmwasm_std::Coin) -> Self {
        Self {
            denom: value.denom,
            amount: value.amount.to_string(),
        }
    }
}

pub fn redeem_token_for_share_msg(
    delegator: impl Into<String>,
    token: cosmwasm_std::Coin,
) -> ProtobufAny {
    #[derive(Clone, PartialEq, Message)]
    struct MsgRedeemTokenForShare {
        #[prost(string, tag = "1")]
        delegator_address: String,
        #[prost(message, optional, tag = "2")]
        amount: Option<RawCoin>,
    }

    fn build_msg(delegator_address: String, raw_coin: RawCoin) -> ProtobufAny {
        let msg = MsgRedeemTokenForShare {
            delegator_address,
            amount: Some(raw_coin),
        };

        let encoded = msg.encode_to_vec();

        ProtobufAny {
            type_url: "/cosmos.staking.v1beta1.MsgRedeemTokensForShares".to_string(),
            value: encoded.into(),
        }
    }

    build_msg(delegator.into(), token.into())
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct DenomTrace {
    pub path: String,
    pub base_denom: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct QueryDenomTraceResponse {
    pub denom_trace: DenomTrace,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDenomTraceRequest {
    #[prost(string, tag = "1")]
    pub hash: ::prost::alloc::string::String,
}

pub fn query_denom_trace(
    deps: Deps<NeutronQuery>,
    hash: String,
) -> StdResult<QueryDenomTraceResponse> {
    let req = QueryRequest::Stargate {
        path: "/ibc.applications.transfer.v1.Query/DenomTrace".to_owned(),
        data: QueryDenomTraceRequest {
            hash: hash.to_string(),
        }
        .encode_to_vec()
        .into(),
    };
    let denom_trace: QueryDenomTraceResponse = deps.querier.query(&req.into())?;
    return Ok(denom_trace);
}

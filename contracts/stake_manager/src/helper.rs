use crate::error_conversion::ContractError;
use crate::query_callback::register_query_submsg;
use crate::state::{IcaInfo, PoolInfo, QueryKind, SudoPayload, TxType, POOLS};
use crate::state::{ADDRESS_TO_REPLY_ID, INFO_OF_ICA_ID, REPLY_ID_TO_QUERY_ID};
use crate::tx_callback::msg_with_sudo_callback;
use cosmos_sdk_proto::cosmos::base::v1beta1::Coin;
use cosmos_sdk_proto::cosmos::distribution::v1beta1::MsgSetWithdrawAddress;
use cosmos_sdk_proto::cosmos::staking::v1beta1::{MsgBeginRedelegate, MsgDelegate};
use cosmos_sdk_proto::prost::Message;
use cosmwasm_std::{instantiate2_address, to_json_binary, WasmMsg};
use cosmwasm_std::{Binary, Deps, DepsMut, QueryRequest, StdResult, Uint128};
use cosmwasm_std::{Env, MessageInfo, Response};
use lsd_token::msg::InstantiateMinterData;
use neutron_sdk::bindings::msg::{IbcFee, NeutronMsg};
use neutron_sdk::bindings::query::NeutronQuery;
use neutron_sdk::bindings::types::{KVKey, ProtobufAny};
use neutron_sdk::interchain_queries::helpers::decode_and_convert;
use neutron_sdk::interchain_queries::v045::helpers::{
    create_delegation_key, create_params_store_key, create_validator_key,
};
use neutron_sdk::interchain_queries::v045::new_register_delegator_delegations_query_msg;
use neutron_sdk::interchain_queries::v045::types::{
    KEY_BOND_DENOM, PARAMS_STORE_KEY, STAKING_STORE_KEY,
};
use neutron_sdk::interchain_queries::v045::{
    new_register_balance_query_msg, new_register_staking_validators_query_msg,
};
use neutron_sdk::NeutronError;
use neutron_sdk::{query::min_ibc_fee::query_min_ibc_fee, NeutronResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const FEE_DENOM: &str = "untrn";
pub const ICA_WITHDRAW_SUFIX: &str = "-withdraw_addr";
pub const INTERCHAIN_ACCOUNT_ID_LEN_LIMIT: usize = 10;
pub const CAL_BASE: Uint128 = Uint128::new(1_000_000);
pub const DEFAULT_DECIMALS: u8 = 6;
pub const DEFAULT_ERA_SECONDS: u64 = 86400; //24h
pub const MIN_ERA_SECONDS: u64 = 28800; //8h

// Default timeout for SubmitTX is 30h
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 30 * 60 * 60;
pub const DEFAULT_UPDATE_PERIOD: u64 = 12000;
pub const DEFAULT_FAST_PERIOD: u64 = 12;

pub const REPLY_ID_RANGE_START: u64 = 1_000_000_000;
pub const REPLY_ID_RANGE_SIZE: u64 = 1_000_000;
pub const REPLY_ID_RANGE_END: u64 = REPLY_ID_RANGE_START + REPLY_ID_RANGE_SIZE;

pub const QUERY_REPLY_ID_RANGE_START: u64 = 2_000_000_000;
pub const QUERY_REPLY_ID_RANGE_SIZE: u64 = 1_000_000;
pub const QUERY_REPLY_ID_RANGE_END: u64 = QUERY_REPLY_ID_RANGE_START + QUERY_REPLY_ID_RANGE_SIZE;

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

pub fn query_denom_trace_from_ibc_denom(
    deps: Deps<NeutronQuery>,
    ibc_denom: String,
) -> StdResult<QueryDenomTraceResponse> {
    let denom_parts: Vec<String> = ibc_denom.split("/").map(String::from).collect();
    if denom_parts.len() != 2 {
        return Err(ContractError::DenomNotMatch {}.into());
    }

    let denom_hash = denom_parts.get(1).unwrap();

    let req = QueryRequest::Stargate {
        path: "/ibc.applications.transfer.v1.Query/DenomTrace".to_owned(),
        data: QueryDenomTraceRequest {
            hash: denom_hash.to_string(),
        }
        .encode_to_vec()
        .into(),
    };
    let denom_trace: QueryDenomTraceResponse = deps.querier.query(&req.into())?;
    return Ok(denom_trace);
}

pub fn get_query_id(
    deps: Deps<NeutronQuery>,
    addr: String,
    query_kind: QueryKind,
) -> StdResult<u64> {
    let reply_id = ADDRESS_TO_REPLY_ID.load(deps.storage, (addr, query_kind.to_string()))?;
    let query_id = REPLY_ID_TO_QUERY_ID.load(deps.storage, reply_id)?;
    return Ok(query_id);
}

pub fn get_update_pool_icq_msgs(
    deps: DepsMut<NeutronQuery>,
    pool_addr: String,
    pool_ica_id: String,
    period: u64,
) -> Result<Vec<NeutronMsg>, NeutronError> {
    let mut msgs = vec![];
    let pool_balances_query_id =
        get_query_id(deps.as_ref(), pool_addr.clone(), QueryKind::Balances)?;

    let (_, withdraw_ica_info, _) = INFO_OF_ICA_ID.load(deps.storage, pool_ica_id)?;
    let withdraw_addr_balances_query_id = get_query_id(
        deps.as_ref(),
        withdraw_ica_info.ica_addr,
        QueryKind::Balances,
    )?;

    let pool_delegations_query_id =
        get_query_id(deps.as_ref(), pool_addr.clone(), QueryKind::Delegations)?;
    let pool_validators_query_id =
        get_query_id(deps.as_ref(), pool_addr.clone(), QueryKind::Validators)?;

    let update_pool_balances_msg =
        NeutronMsg::update_interchain_query(pool_balances_query_id, None, Some(period), None)?;
    let update_withdraw_addr_balances_msg = NeutronMsg::update_interchain_query(
        withdraw_addr_balances_query_id,
        None,
        Some(period),
        None,
    )?;
    let update_pool_delegations_msg =
        NeutronMsg::update_interchain_query(pool_delegations_query_id, None, Some(period), None)?;
    let update_pool_validators_msg =
        NeutronMsg::update_interchain_query(pool_validators_query_id, None, Some(period), None)?;

    msgs.push(update_pool_balances_msg);
    msgs.push(update_withdraw_addr_balances_msg);
    msgs.push(update_pool_delegations_msg);
    msgs.push(update_pool_validators_msg);
    Ok(msgs)
}

pub fn deal_pool(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    mut pool_info: PoolInfo,
    pool_ica_info: IcaInfo,
    withdraw_ica_info: IcaInfo,
    lsd_code_id: u64,
    lsd_token_name: String,
    lsd_token_symbol: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let denom_trace = query_denom_trace_from_ibc_denom(deps.as_ref(), pool_info.ibc_denom.clone())?;
    if denom_trace.denom_trace.base_denom != pool_info.remote_denom {
        return Err(ContractError::DenomTraceNotMatch {}.into());
    }

    let salt = &pool_ica_info.ica_addr.clone()[..40];
    let code_info = deps.querier.query_wasm_code_info(lsd_code_id)?;
    let creator_cannonical = deps.api.addr_canonicalize(env.contract.address.as_str())?;
    let i2_address =
        instantiate2_address(&code_info.checksum, &creator_cannonical, salt.as_bytes())
            .map_err(|e| ContractError::Instantiate2AddressFailed(e.to_string()))?;
    let contract_addr = deps
        .api
        .addr_humanize(&i2_address)
        .map_err(NeutronError::Std)?;

    pool_info.lsd_token = contract_addr;

    let instantiate_lsd_msg = WasmMsg::Instantiate2 {
        admin: Option::from(info.sender.to_string()),
        code_id: lsd_code_id,
        msg: to_json_binary(
            &(lsd_token::msg::InstantiateMsg {
                name: lsd_token_name.clone(),
                symbol: lsd_token_symbol,
                decimals: DEFAULT_DECIMALS,
                initial_balances: vec![],
                mint: Option::from(InstantiateMinterData {
                    admin: pool_info.admin.to_string(),
                    minter: env.contract.address.to_string(),
                    cap: None,
                }),
                marketing: None,
            }),
        )?,
        funds: vec![],
        label: lsd_token_name.clone(),
        salt: salt.as_bytes().into(),
    };

    POOLS.save(deps.storage, pool_ica_info.ica_addr.clone(), &pool_info)?;

    let register_balance_pool_submsg = register_query_submsg(
        deps.branch(),
        new_register_balance_query_msg(
            pool_ica_info.ctrl_connection_id.clone(),
            pool_ica_info.ica_addr.clone(),
            pool_info.remote_denom.clone(),
            DEFAULT_UPDATE_PERIOD,
        )?,
        pool_ica_info.ica_addr.clone(),
        QueryKind::Balances,
    )?;
    let register_balance_withdraw_submsg = register_query_submsg(
        deps.branch(),
        new_register_balance_query_msg(
            withdraw_ica_info.ctrl_connection_id.clone(),
            withdraw_ica_info.ica_addr.clone(),
            pool_info.remote_denom.clone(),
            DEFAULT_UPDATE_PERIOD,
        )?,
        withdraw_ica_info.ica_addr.clone(),
        QueryKind::Balances,
    )?;
    let register_delegation_submsg = register_query_submsg(
        deps.branch(),
        new_register_delegator_delegations_query_msg(
            pool_ica_info.ctrl_connection_id.clone(),
            pool_ica_info.ica_addr.clone(),
            pool_info.validator_addrs.clone(),
            DEFAULT_UPDATE_PERIOD,
        )?,
        pool_ica_info.ica_addr.clone(),
        QueryKind::Delegations,
    )?;

    let register_validator_submsg = register_query_submsg(
        deps.branch(),
        new_register_staking_validators_query_msg(
            pool_ica_info.ctrl_connection_id.clone(),
            pool_info.validator_addrs.clone(),
            6,
        )?,
        pool_ica_info.ica_addr.clone(),
        QueryKind::Validators,
    )?;

    let set_withdraw_msg = MsgSetWithdrawAddress {
        delegator_address: pool_ica_info.ica_addr.clone(),
        withdraw_address: withdraw_ica_info.ica_addr.clone(),
    };
    let mut buf = Vec::new();
    buf.reserve(set_withdraw_msg.encoded_len());

    if let Err(e) = set_withdraw_msg.encode(&mut buf) {
        return Err(ContractError::EncodeError(e.to_string()).into());
    }

    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let cosmos_msg = NeutronMsg::submit_tx(
        pool_ica_info.ctrl_connection_id.clone(),
        pool_info.ica_id.clone(),
        vec![ProtobufAny {
            type_url: "/cosmos.distribution.v1beta1.MsgSetWithdrawAddress".to_string(),
            value: Binary::from(buf),
        }],
        "".to_string(),
        DEFAULT_TIMEOUT_SECONDS,
        fee.clone(),
    );

    // We use a submessage here because we need the process message reply to save
    // the outgoing IBC packet identifier for later.
    let submsg_set_withdraw = msg_with_sudo_callback(
        deps.branch(),
        cosmos_msg,
        SudoPayload {
            port_id: pool_ica_info.ctrl_port_id,
            message: withdraw_ica_info.ica_addr,
            pool_addr: pool_ica_info.ica_addr.clone(),
            tx_type: TxType::SetWithdrawAddr,
        },
    )?;

    Ok(Response::default()
        .add_message(instantiate_lsd_msg)
        .add_submessages(vec![
            register_balance_pool_submsg,
            register_balance_withdraw_submsg,
            register_delegation_submsg,
            register_validator_submsg,
            submsg_set_withdraw,
        ]))
}

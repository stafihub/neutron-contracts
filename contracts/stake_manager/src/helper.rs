use crate::query_callback::register_query_submsg;
use crate::state::{
    IcaInfo, PoolInfo, QueryKind, SudoPayload, TxType, ERA_RATE, POOLS, TOTAL_STACK_FEE,
};
use crate::state::{ADDRESS_TO_REPLY_ID, INFO_OF_ICA_ID, REPLY_ID_TO_QUERY_ID};
use crate::tx_callback::msg_with_sudo_callback;
use crate::{error_conversion::ContractError, state::EraStatus};
use cosmos_sdk_proto::cosmos::bank::v1beta1::MsgSend;
use cosmos_sdk_proto::cosmos::base::v1beta1::Coin;
use cosmos_sdk_proto::cosmos::distribution::v1beta1::MsgSetWithdrawAddress;
use cosmos_sdk_proto::cosmos::staking::v1beta1::{MsgBeginRedelegate, MsgDelegate};
use cosmos_sdk_proto::prost::Message;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{instantiate2_address, to_json_binary, SubMsg, Uint64, WasmMsg};
use cosmwasm_std::{Binary, Deps, DepsMut, QueryRequest, StdResult, Uint128};
use cosmwasm_std::{Env, MessageInfo, Response};
use lsd_token::msg::InstantiateMinterData;
use neutron_sdk::bindings::msg::{IbcFee, NeutronMsg};
use neutron_sdk::bindings::query::NeutronQuery;
use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::interchain_queries::v045::new_register_delegator_delegations_query_msg;
use neutron_sdk::interchain_queries::v045::{
    new_register_balance_query_msg, new_register_staking_validators_query_msg,
};
use neutron_sdk::interchain_queries::v047::register_queries::new_register_delegator_delegations_query_msg as v047_new_register_delegator_delegations_query_msg;
use neutron_sdk::NeutronError;
use neutron_sdk::{query::min_ibc_fee::query_min_ibc_fee, NeutronResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ops::Add;

pub const FEE_DENOM: &str = "untrn";
pub const ICA_WITHDRAW_SUFIX: &str = "-withdraw_addr";
pub const INTERCHAIN_ACCOUNT_ID_LEN_LIMIT: usize = 10;
pub const CAL_BASE: Uint128 = Uint128::new(1_000_000);
pub const DEFAULT_RATE: Uint128 = Uint128::new(1_000_000);
pub const DEFAULT_DECIMALS: u8 = 6;
pub const DEFAULT_ERA_SECONDS: u64 = 86400; //24h
pub const MIN_ERA_SECONDS: u64 = 28800; //8h
pub const MAX_ERA_SECONDS: u64 = 86400; //24h

pub const STAKE_SPLIT_THRESHOLD: Uint128 = Uint128::new(10_000);

// Default timeout for SubmitTX is 30h
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 30 * 60 * 60;
pub const DEFAULT_UPDATE_PERIOD: u64 = 86400;
pub const DEFAULT_FAST_PERIOD: u64 = 60;

pub const UNBONDING_SECONDS: u64 = 1814400;

pub const REPLY_ID_RANGE_START: u64 = 1_000_000_000;
pub const REPLY_ID_RANGE_SIZE: u64 = 1_000_000;
pub const REPLY_ID_RANGE_END: u64 = REPLY_ID_RANGE_START + REPLY_ID_RANGE_SIZE;

pub const QUERY_REPLY_ID_RANGE_START: u64 = 2_000_000_000;
pub const QUERY_REPLY_ID_RANGE_SIZE: u64 = 1_000_000;
pub const QUERY_REPLY_ID_RANGE_END: u64 = QUERY_REPLY_ID_RANGE_START + QUERY_REPLY_ID_RANGE_SIZE;

pub fn min_ntrn_ibc_fee(fee: IbcFee) -> IbcFee {
    IbcFee {
        recv_fee: fee
            .recv_fee
            .into_iter()
            .filter(|a| a.denom == FEE_DENOM)
            .collect(),
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

pub fn total_ibc_fee(ibc_fee: IbcFee) -> Uint128 {
    let recv_fee = if ibc_fee.recv_fee.len() > 0 {
        ibc_fee.recv_fee[0].amount
    } else {
        Uint128::zero()
    };
    let ack_fee = if ibc_fee.ack_fee.len() > 0 {
        ibc_fee.ack_fee[0].amount
    } else {
        Uint128::zero()
    };
    let time_out_fee = if ibc_fee.timeout_fee.len() > 0 {
        ibc_fee.timeout_fee[0].amount
    } else {
        Uint128::zero()
    };

    recv_fee.add(ack_fee).add(time_out_fee)
}

pub fn check_ibc_fee(deps: Deps<NeutronQuery>, info: &MessageInfo) -> NeutronResult<IbcFee> {
    let ibc_fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps)?.min_fee);
    let total_ibc_fee = total_ibc_fee(ibc_fee.clone());
    if !(info.funds.len() == 1
        && info.funds[0].denom == FEE_DENOM
        && info.funds[0].amount >= total_ibc_fee)
    {
        return Err(ContractError::ParamsErrorFundsNotMatch {}.into());
    }
    return Ok(ibc_fee);
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
    Ok(denom_trace)
}

pub fn get_query_id(
    deps: Deps<NeutronQuery>,
    addr: String,
    query_kind: QueryKind,
) -> StdResult<u64> {
    let reply_id = ADDRESS_TO_REPLY_ID.load(deps.storage, (addr, query_kind.to_string()))?;
    let query_id = REPLY_ID_TO_QUERY_ID.load(deps.storage, reply_id)?;
    Ok(query_id)
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

pub fn register_delegator_delegations_query_msg(
    connection_id: String,
    delegator: String,
    validators: Vec<String>,
    update_period: u64,
    sdk_greater_or_euqal_v47: bool,
) -> NeutronResult<NeutronMsg> {
    if sdk_greater_or_euqal_v47 {
        v047_new_register_delegator_delegations_query_msg(
            connection_id,
            delegator,
            validators,
            update_period,
        )
    } else {
        new_register_delegator_delegations_query_msg(
            connection_id,
            delegator,
            validators,
            update_period,
        )
    }
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
    pool_info.status = EraStatus::InitStarted;

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
    ERA_RATE.save(
        deps.storage,
        (pool_ica_info.ica_addr.clone(), pool_info.era),
        &pool_info.rate,
    )?;
    TOTAL_STACK_FEE.save(
        deps.storage,
        pool_ica_info.ica_addr.clone(),
        &Uint128::zero(),
    )?;

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
        register_delegator_delegations_query_msg(
            pool_ica_info.ctrl_connection_id.clone(),
            pool_ica_info.ica_addr.clone(),
            pool_info.validator_addrs.clone(),
            DEFAULT_UPDATE_PERIOD,
            pool_info.sdk_greater_or_equal_v047,
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

    let mut sub_msgs = vec![];
    sub_msgs.push(register_balance_pool_submsg);
    sub_msgs.push(register_balance_withdraw_submsg);
    sub_msgs.push(register_delegation_submsg);
    sub_msgs.push(register_validator_submsg);
    sub_msgs.push(set_withdraw_sub_msg(
        deps,
        pool_info,
        pool_ica_info,
        withdraw_ica_info,
    )?);

    Ok(Response::default()
        .add_message(instantiate_lsd_msg)
        .add_submessages(sub_msgs))
}

pub fn set_withdraw_sub_msg(
    mut deps: DepsMut<NeutronQuery>,
    pool_info: PoolInfo,
    pool_ica_info: IcaInfo,
    withdraw_ica_info: IcaInfo,
) -> NeutronResult<SubMsg<NeutronMsg>> {
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
    Ok(submsg_set_withdraw)
}

pub fn sudo_set_withdraw_addr_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;

    pool_info.status = EraStatus::ActiveEnded;

    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;

    Ok(Response::new())
}

pub fn sudo_set_withdraw_addr_failed_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;

    pool_info.status = EraStatus::InitFailed;

    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;

    Ok(Response::new())
}

pub fn deal_validators_icq_update(
    deps: DepsMut<NeutronQuery>,
    pool_addr: String,
    pool_info: PoolInfo,
    ctrl_connection_id: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let new_delegations_keys = match register_delegator_delegations_query_msg(
        ctrl_connection_id.clone(),
        pool_addr.clone(),
        pool_info.validator_addrs.clone(),
        DEFAULT_UPDATE_PERIOD,
        pool_info.sdk_greater_or_equal_v047,
    ) {
        Ok(NeutronMsg::RegisterInterchainQuery { keys, .. }) => keys,
        _ => return Err(ContractError::ICQNewKeyBuildFailed {}.into()),
    };

    let update_pool_delegations_msg = NeutronMsg::update_interchain_query(
        get_query_id(deps.as_ref(), pool_addr.clone(), QueryKind::Delegations)?,
        Some(new_delegations_keys),
        None,
        None,
    )?;

    let new_validators_keys = match new_register_staking_validators_query_msg(
        ctrl_connection_id,
        pool_info.validator_addrs.clone(),
        DEFAULT_UPDATE_PERIOD,
    ) {
        Ok(NeutronMsg::RegisterInterchainQuery { keys, .. }) => keys,
        _ => return Err(ContractError::ICQNewKeyBuildFailed {}.into()),
    };

    let update_pool_validators_msg = NeutronMsg::update_interchain_query(
        get_query_id(deps.as_ref(), pool_addr.clone(), QueryKind::Validators)?,
        Some(new_validators_keys),
        None,
        None,
    )?;

    Ok(Response::default().add_messages(vec![
        update_pool_delegations_msg,
        update_pool_validators_msg,
    ]))
}

#[cw_serde]
struct IcqParams {
    query_submit_timeout: Uint64,
    query_deposit: Vec<cosmwasm_std::Coin>,
    tx_query_removal_limit: Uint64,
}

impl IcqParams {
    const TYPE_URL: &'static str = "/neutron.interchainqueries.Query/Params";
}

#[cw_serde]
struct QueryIcqParamsResponse {
    params: IcqParams,
}

pub fn query_icq_register_fee(deps: Deps<NeutronQuery>) -> StdResult<Vec<cosmwasm_std::Coin>> {
    let res: QueryIcqParamsResponse = deps.querier.query(&QueryRequest::Stargate {
        path: IcqParams::TYPE_URL.to_owned(),
        data: Binary(vec![]),
    })?;

    let coin = res.params.query_deposit;
    Ok(coin)
}

pub fn total_icq_register_fee(fee: Vec<cosmwasm_std::Coin>) -> Uint128 {
    let filter_fee: Vec<cosmwasm_std::Coin> =
        fee.into_iter().filter(|a| a.denom == FEE_DENOM).collect();

    if filter_fee.len() > 0 {
        filter_fee[0].amount
    } else {
        Uint128::zero()
    }
}

pub fn gen_msg_send(
    from_address: String,
    to_address: String,
    denom: String,
    amount: String,
) -> NeutronResult<ProtobufAny> {
    // interchain tx send atom
    let ica_send = MsgSend {
        from_address,
        to_address,
        amount: vec![Coin { denom, amount }],
    };
    let mut buf = Vec::new();
    buf.reserve(ica_send.encoded_len());

    ica_send
        .encode(&mut buf)
        .map_err(|e| ContractError::EncodeError(e.to_string()))?;

    Ok(ProtobufAny {
        type_url: "/cosmos.bank.v1beta1.MsgSend".to_string(),
        value: Binary::from(buf),
    })
}

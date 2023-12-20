use std::ops::{Add, Mul};
use cosmwasm_std::{
	coin,
	to_json_binary,
	entry_point,
	from_json,
	Binary,
	CosmosMsg,
	Deps,
	DepsMut,
	Env,
	MessageInfo,
	Reply,
	Response,
	StdError,
	StdResult,
	SubMsg,
	Uint128,
	WasmMsg,
	CustomQuery,
	Addr,
	Int256,
};
use cw2::set_contract_version;
use neutron_sdk::{
	bindings::{msg::{IbcFee, MsgIbcTransferResponse, NeutronMsg}, query::NeutronQuery},
	query::min_ibc_fee::query_min_ibc_fee,
	sudo::msg::{RequestPacket, RequestPacketTimeoutHeight},
	NeutronResult,
	NeutronError,
};
use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::interchain_txs::helpers::get_port_id;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cw20_ratom::{msg};
use cosmos_sdk_proto::cosmos::bank::v1beta1::{MsgSend};
use neutron_sdk::interchain_queries::{v045::{new_register_delegator_delegations_query_msg}};
use cosmos_sdk_proto::cosmos::base::v1beta1::Coin;
use cosmos_sdk_proto::cosmos::staking::v1beta1::{MsgDelegate, MsgUndelegate};
use cosmos_sdk_proto::prost::Message;
use neutron_sdk::interchain_queries::v045::new_register_balance_query_msg;
use neutron_sdk::sudo::msg::SudoMsg;
use crate::{
	msg::{ExecuteMsg, InstantiateMsg, MigrateMsg},
	state::{
		read_reply_payload,
		read_sudo_payload,
		save_reply_payload,
		save_sudo_payload,
		IBC_SUDO_ID_RANGE_END,
		IBC_SUDO_ID_RANGE_START,
		CONNECTION_POOL_MAP,
	},
};
use crate::state::{
	INTERCHAIN_ACCOUNTS,
	STATE,
	State,
	UnstakeInfo,
	UNSTAKES_INDEX_FOR_USER,
	UNSTAKES_OF_INDEX,
	POOLS,
	POOL_DENOM_MPA,
	POOL_ERA_INFO,
	POOL_ICA_MAP,
};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
struct OpenAckVersion {
	version: String,
	controller_connection_id: String,
	host_connection_id: String,
	address: String,
	encoding: String,
	tx_type: String,
}

// Default timeout for IbcTransfer is 10000000 blocks
const DEFAULT_TIMEOUT_HEIGHT: u64 = 10000000;

pub const SUDO_PAYLOAD_REPLY_ID: u64 = 1;

// Default timeout for SubmitTX is two weeks
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 60 * 60 * 24 * 7 * 2;

// config by instantiate
// const UATOM_IBC_DENOM: &str =
// 	"ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2";

const FEE_DENOM: &str = "untrn";

const CONTRACT_NAME: &str = concat!("crates.io:neutron-sdk__", env!("CARGO_PKG_NAME"));
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// too
#[entry_point]
pub fn instantiate(
	deps: DepsMut,
	_: Env,
	info: MessageInfo,
	msg: InstantiateMsg,
) -> NeutronResult<Response<NeutronMsg>> {
	set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

	STATE.save(
		deps.storage,
		&(State {
			owner: info.sender,
			minimal_stake: msg.minimal_stake,
			cw20: msg.cw20_address,
			unstake_times_limit: msg.unstake_times_limit,
			next_unstake_index: Uint128::zero(),
			unbonding_period: msg.unbonding_period,
		}),
	)?;

	// todo: move to new entry point
	// ERA.save(
	// 	deps.storage,
	// 	&(Era {
	// 		era: msg.era,
	// 		pre_era: msg.era - 1,
	// 		rate: msg.rate,
	// 		pre_rate: msg.rate,
	// 		era_update_status: true,
	// 	}),
	// )?;

	Ok(Response::new())
}

// todo: add response event
// todo: add execute add pool
#[entry_point]
pub fn execute(
	deps: DepsMut<NeutronQuery>,
	env: Env,
	info: MessageInfo,
	msg: ExecuteMsg,
) -> NeutronResult<Response<NeutronMsg>> {
	match msg {
		// NOTE: this is an example contract that shows how to make IBC transfers!
		// Please add necessary authorization or other protection mechanisms
		// if you intend to send funds over IBC
		ExecuteMsg::RegisterPool { connection_id, interchain_account_id } =>
			execute_register_pool(deps, env, info, connection_id, interchain_account_id),
		ExecuteMsg::RegisterBalanceQuery {
			connection_id,
			addr,
			denom,
			update_period,
		} => register_balance_query(connection_id, addr, denom, update_period),
		ExecuteMsg::RegisterDelegatorDelegationsQuery {
			connection_id,
			delegator,
			validators,
			update_period,
		} => register_delegations_query(connection_id, delegator, validators, update_period),
		ExecuteMsg::Stake { neutron_address, pool_addr } =>
			execute_stake(deps, env, neutron_address, pool_addr, info),
		ExecuteMsg::Unstake { amount, interchain_account_id, pool_addr } =>
			execute_unstake(deps, env, info, amount, interchain_account_id, pool_addr),
		ExecuteMsg::Withdraw { pool_addr, receiver, interchain_account_id } =>
			execute_withdraw(deps, env, info, pool_addr, receiver, interchain_account_id),
		ExecuteMsg::EraUpdate { connection_id, channel } =>
		// Different rtoken are executed separately.
			execute_era_update(deps, env, info.funds, connection_id, channel),
		ExecuteMsg::StakeLSM {} => execute_stake_lsm(deps, env, info),
	}
}

// Example of different payload types
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct IbcSendType {
	pub message: String,
}

// a callback handler for payload of IbcSendType
fn sudo_ibc_send_callback(deps: Deps, payload: IbcSendType) -> StdResult<Response> {
	deps.api.debug(format!("WASMDEBUG: callback: ibc send sudo payload: {:?}", payload).as_str());
	Ok(Response::new())
}

// a callback handler for payload of InterTxType
fn sudo_inter_tx_callback(deps: Deps, payload: InterTxType) -> StdResult<Response> {
	deps.api.debug(format!("WASMDEBUG: callback: inter tx sudo payload: {:?}", payload).as_str());
	Ok(Response::new())
}

// Enum representing payload to process during handling acknowledgement messages in Sudo handler
#[derive(Serialize, Deserialize)]
pub enum SudoPayload {
	HandlerPayloadIbcSend(IbcSendType),
	HandlerPayloadInterTx(InterTxType),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InterTxType {
	pub message: String,
	pub port_id: String,
}

// saves payload to process later to the storage and returns a SubmitTX Cosmos SubMsg with necessary reply id
fn msg_with_sudo_callback<C: Into<CosmosMsg<T>>, T>(
	deps: DepsMut<NeutronQuery>,
	msg: C,
	payload: SudoPayload,
) -> StdResult<SubMsg<T>> {
	let id = save_reply_payload(deps.storage, payload)?;
	Ok(SubMsg::reply_on_success(msg, id))
}

// prepare_sudo_payload is called from reply handler
// The method is used to extract sequence id and channel from SubmitTxResponse to process sudo payload defined in msg_with_sudo_callback later in Sudo handler.
// Such flow msg_with_sudo_callback() -> reply() -> prepare_sudo_payload() -> sudo() allows you "attach" some payload to your Transfer message
// and process this payload when an acknowledgement for the SubmitTx message is received in Sudo handler
fn prepare_sudo_payload(mut deps: DepsMut, _env: Env, msg: Reply) -> StdResult<Response> {
	let payload = read_reply_payload(deps.storage, msg.id)?;
	let resp: MsgIbcTransferResponse = from_json(
		&msg.result
			.into_result()
			.map_err(StdError::generic_err)?
			.data.ok_or_else(|| StdError::generic_err("no result"))?
	).map_err(|e| StdError::generic_err(format!("failed to parse response: {:?}", e)))?;
	let seq_id = resp.sequence_id;
	let channel_id = resp.channel;
	save_sudo_payload(deps.branch().storage, channel_id, seq_id, payload)?;
	Ok(Response::new())
}

#[entry_point]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> StdResult<Response> {
	match msg.id {
		// It's convenient to use range of ID's to handle multiple reply messages
		IBC_SUDO_ID_RANGE_START..=IBC_SUDO_ID_RANGE_END => prepare_sudo_payload(deps, env, msg),
		_ => Err(StdError::generic_err(format!("unsupported reply message id {}", msg.id))),
	}
}

// todo: refactor to add pool
fn execute_register_pool(
	deps: DepsMut<NeutronQuery>,
	env: Env,
	_: MessageInfo,
	connection_id: String,
	interchain_account_id: String,
) -> NeutronResult<Response<NeutronMsg>> {
	let register = NeutronMsg::register_interchain_account(
		connection_id.clone(),
		interchain_account_id.clone(),
	);
	let key = get_port_id(env.contract.address.as_str(), &interchain_account_id);
	// we are saving empty data here because we handle response of registering ICA in sudo_open_ack method
	INTERCHAIN_ACCOUNTS.save(deps.storage, key, &None)?;

	Ok(Response::default().add_message(register))
}

// todo: add execute to config the validator addrs and withdraw address on reply
fn execute_config_pool(
	deps: DepsMut<NeutronQuery>,
	env: Env,
	_: MessageInfo,
	connection_id: String,
	interchain_account_id: String,
) -> NeutronResult<Response<NeutronMsg>> {
	let (delegator, connection_id) = get_ica(deps.as_ref(), &env, &interchain_account_id)?;


	Ok(Response::default())
}

pub fn register_delegations_query(
	connection_id: String,
	delegator: String,
	validators: Vec<String>,
	update_period: u64,
) -> NeutronResult<Response<NeutronMsg>> {
	let msg = new_register_delegator_delegations_query_msg(
		connection_id,
		delegator,
		validators,
		update_period,
	)?;

	Ok(Response::new().add_message(msg))
}

pub fn register_balance_query(
	connection_id: String,
	addr: String,
	denom: String,
	update_period: u64,
) -> NeutronResult<Response<NeutronMsg>> {
	let msg = new_register_balance_query_msg(connection_id, addr, denom, update_period)?;

	Ok(Response::new().add_message(msg))
}

fn execute_stake(
	deps: DepsMut<NeutronQuery>,
	_: Env,
	neutron_address: String,
	pool_addr: String,
	info: MessageInfo,
) -> NeutronResult<Response<NeutronMsg>> {
	let state = STATE.load(deps.storage)?;
	let era_info = POOL_ERA_INFO.load(deps.storage, pool_addr.clone())?;
	let pool_denom = POOL_DENOM_MPA.load(deps.storage, pool_addr.clone())?;

	let mut amount = 0;
	if !info.funds.is_empty() {
		amount = u128::from(
			info.funds
				.iter()
				.find(|c| c.denom == pool_denom)
				.map(|c| c.amount)
				.unwrap_or(Uint128::zero())
		);
	}

	// todo: calu rate
	amount = amount.mul(era_info.rate.u128());

	let msg = WasmMsg::Execute {
		contract_addr: state.cw20.to_string(),
		msg: to_json_binary(
			&(msg::ExecuteMsg::Mint {
				recipient: neutron_address.to_string(),
				amount: Uint128::from(amount),
			})
		)?,
		funds: vec![],
	};

	Ok(Response::new().add_message(CosmosMsg::Wasm(msg)).add_attribute("mint", "call_contract_b"))
}

// Before this step, need the user to authorize burn from
fn execute_unstake(
	deps: DepsMut<NeutronQuery>,
	env: Env,
	info: MessageInfo,
	amount: Uint128,
	interchain_account_id: String,
	pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
	if amount == Uint128::zero() {
		return Err(
			NeutronError::Std(
				StdError::generic_err(format!("Encode error: {}", "LSD token amount is zero"))
			)
		);
	}

	let mut state = STATE.load(deps.storage)?;
	let era_info = POOL_ERA_INFO.load(deps.storage, pool_addr.clone())?;

	let unstake_count = UNSTAKES_INDEX_FOR_USER.load(deps.storage, &info.sender)?.len() as u128;
	let unstake_limit = state.unstake_times_limit.u128();
	if unstake_count >= unstake_limit {
		return Err(
			NeutronError::Std(
				StdError::generic_err(format!("Encode error: {}", "Unstake times limit reached"))
			)
		);
	}

	// Calculate the number of tokens(atom)
	let token_amount = amount.mul(era_info.rate);

	let (delegator, _) = get_ica(deps.as_ref(), &env, &interchain_account_id)?;

	// update pool info
	let mut pools = POOLS.load(deps.storage, pool_addr.clone())?;
	pools.unbond += Int256::from(token_amount);
	pools.active -= Int256::from(token_amount);
	POOLS.save(deps.storage, pool_addr, &pools)?;

	// update unstake info
	let unstake_info = UnstakeInfo {
		era: era_info.era,
		pool: delegator,
		amount: token_amount,
	};

	let will_use_unstake_index = state.next_unstake_index;

	UNSTAKES_OF_INDEX.save(deps.storage, will_use_unstake_index.u128(), &unstake_info)?;

	state.next_unstake_index = state.next_unstake_index.add(Uint128::one());
	STATE.save(deps.storage, &state)?;

	// burn
	let msg = WasmMsg::Execute {
		contract_addr: state.cw20.to_string(),
		msg: to_json_binary(
			&(msg::ExecuteMsg::BurnFrom {
				owner: info.sender.to_string(),
				amount: Default::default(),
			})
		)?,
		funds: vec![],
	};

	// send event
	Ok(
		Response::new()
			.add_message(CosmosMsg::Wasm(msg))
			.add_attribute("action", "unstake")
			.add_attribute("from", info.sender)
			.add_attribute("token_amount", token_amount.to_string())
			.add_attribute("lsd_token_amount", amount.to_string())
			.add_attribute("unstake_index", will_use_unstake_index.to_string())
	)
}

fn execute_withdraw(
	mut deps: DepsMut<NeutronQuery>,
	env: Env,
	info: MessageInfo,
	pool_addr: String,
	receiver: Addr,
	interchain_account_id: String,
) -> NeutronResult<Response<NeutronMsg>> {
	let mut total_withdraw_amount = Uint128::zero();
	let mut unstakes = UNSTAKES_INDEX_FOR_USER.load(deps.storage, &info.sender)?;

	let mut emit_unstake_index_list = vec![];
	let mut indices_to_remove = Vec::new();

	let state = STATE.load(deps.storage)?;
	let era_info = POOL_ERA_INFO.load(deps.storage, pool_addr.clone())?;
	let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

	for (i, unstake_index) in unstakes.iter().enumerate() {
		let unstake_info = UNSTAKES_OF_INDEX.load(deps.storage, unstake_index.u128())?;
		if
		unstake_info.era + state.unbonding_period > era_info.era ||
			unstake_info.pool != pool_addr
		{
			continue;
		}

		// Remove the unstake index element of info.sender from UNSTAKES_INDEX_FOR_USER
		total_withdraw_amount += unstake_info.amount;
		emit_unstake_index_list.push(*unstake_index);
		indices_to_remove.push(i);
	}

	// Reverse sort the indices to remove to avoid shifting issues during removal
	indices_to_remove.sort_unstable_by(|a, b| b.cmp(a));

	// Remove the elements
	for index in indices_to_remove {
		unstakes.remove(index);
	}

	UNSTAKES_INDEX_FOR_USER.save(deps.storage, &info.sender, &unstakes)?;

	if total_withdraw_amount.is_zero() {
		return Err(
			NeutronError::Std(
				StdError::generic_err(format!("Encode error: {}", "Zero withdraw amount"))
			)
		);
	}

	let unstake_index_list_str = emit_unstake_index_list
		.iter()
		.map(|index| index.u128().to_string())
		.collect::<Vec<String>>()
		.join(",");

	// interchain tx send atom
	let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
	let (delegator, connection_id) = get_ica(deps.as_ref(), &env, &interchain_account_id)?;
	let ica_send = MsgSend {
		from_address: delegator,
		to_address: receiver.to_string(),
		amount: Vec::from([
			Coin {
				denom: pool_info.ibc_denom,
				amount: total_withdraw_amount.to_string(),
			},
		]),
	};
	let mut buf = Vec::new();
	buf.reserve(ica_send.encoded_len());

	if let Err(e) = ica_send.encode(&mut buf) {
		return Err(NeutronError::Std(StdError::generic_err(format!("Encode error: {}", e))));
	}

	let send_msg = ProtobufAny {
		type_url: "/cosmos.bank.v1beta1.MsgSend".to_string(),
		value: Binary::from(buf),
	};

	let cosmos_msg = NeutronMsg::submit_tx(
		connection_id,
		interchain_account_id.clone(),
		vec![send_msg],
		"".to_string(),
		DEFAULT_TIMEOUT_SECONDS,
		fee,
	);

	// We use a submessage here because we need the process message reply to save
	// the outgoing IBC packet identifier for later.
	let submsg = msg_with_sudo_callback(
		deps.branch(),
		cosmos_msg,
		SudoPayload::HandlerPayloadInterTx(InterTxType {
			port_id: get_port_id(env.contract.address.as_str(), &interchain_account_id),
			message: "message".to_string(),
		}),
	)?;

	Ok(
		Response::new()
			.add_attribute("action", "withdraw")
			.add_attribute("from", info.sender)
			.add_attribute("pool", pool_addr.clone())
			.add_attribute("unstake_index_list", unstake_index_list_str)
			.add_attribute("amount", total_withdraw_amount)
			.add_submessages(vec![submsg])
	)
}

fn execute_stake_lsm(
	_: DepsMut<NeutronQuery>,
	_: Env,
	_: MessageInfo,
) -> NeutronResult<Response<NeutronMsg>> {
	// todo!
	Ok(Response::new())
}

fn execute_redelegate(
	_: DepsMut<NeutronQuery>,
	_: Env,
	_: MessageInfo,
) -> NeutronResult<Response<NeutronMsg>> {
	// todo!
	Ok(Response::new())
}

fn execute_era_update(
	mut deps: DepsMut<NeutronQuery>,
	env: Env,
	funds: Vec<cosmwasm_std::Coin>,
	connection_id: String,
	channel: String,
) -> NeutronResult<Response<NeutronMsg>> {
	// --------------------------------------------------------------------------------------------------
	// contract must pay for relaying of acknowledgements
	// See more info here: https://docs.neutron.org/neutron/feerefunder/overview
	let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
	let pool_array = CONNECTION_POOL_MAP.load(deps.storage, connection_id.clone())?;
	let mut msgs = vec![];
	for pool_addr in pool_array {
		let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
		// todo: check era state

		let mut amount = 0;
		if !funds.is_empty() {
			amount = u128::from(
				funds
					.iter()
					.find(|c| c.denom == pool_info.ibc_denom.clone())
					.map(|c| c.amount)
					.unwrap_or(Uint128::zero())
			);
		}

		let tx_coin = coin(amount, pool_info.ibc_denom.clone());

		let msg = NeutronMsg::IbcTransfer {
			source_port: "transfer".to_string(),
			source_channel: channel.clone(),
			sender: env.contract.address.to_string(),
			receiver: pool_addr.clone(),
			token: tx_coin,
			timeout_height: RequestPacketTimeoutHeight {
				revision_number: Some(2),
				revision_height: Some(DEFAULT_TIMEOUT_HEIGHT),
			},
			timeout_timestamp: DEFAULT_TIMEOUT_SECONDS,
			memo: "".to_string(),
			fee: fee.clone(),
		};

		deps.as_ref().api.debug(format!("WASMDEBUG: IbcTransfer msg: {:?}", msg).as_str());

		let submsg = msg_with_sudo_callback(
			deps.branch(),
			msg,
			SudoPayload::HandlerPayloadIbcSend(IbcSendType {
				message: "message".to_string(),
			}),
		)?;
		deps.as_ref().api.debug(
			format!("WASMDEBUG: execute_send: sent submsg: {:?}", submsg).as_str()
		);
		msgs.push(submsg);

		// todo: check withdraw address balance and send it to the pool
	}

	Ok(Response::default().add_submessages(msgs))
}

// todo? What if some of the operations failed when pledged to multiple verifiers?
fn execute_era_bond(
	mut deps: DepsMut<NeutronQuery>,
	env: Env,
	connection_id: String,
) -> NeutronResult<Response<NeutronMsg>> {
	// --------------------------------------------------------------------------------------------------
	// contract must pay for relaying of acknowledgements
	// See more info here: https://docs.neutron.org/neutron/feerefunder/overview
	let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
	let pool_array = CONNECTION_POOL_MAP.load(deps.storage, connection_id.clone())?;
	let mut msgs = vec![];
	for pool_addr in pool_array {
		let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
		// todo: check era state

		let interchain_account_id = POOL_ICA_MAP.load(deps.storage, pool_addr.clone())?;
		if pool_info.unbond - pool_info.active > Int256::from(0) {
			let unbond_amount = pool_info.unbond - pool_info.active;
			// add submessage to unstake
			let delegate_msg = MsgUndelegate {
				delegator_address: pool_addr.clone(),
				validator_address: pool_info.validator_addrs[0].clone(),
				amount: Some(Coin {
					denom: pool_info.ibc_denom.clone(),
					amount: unbond_amount.to_string(),
				}),
			};
			let mut buf = Vec::new();
			buf.reserve(delegate_msg.encoded_len());

			if let Err(e) = delegate_msg.encode(&mut buf) {
				return Err(
					NeutronError::Std(StdError::generic_err(format!("Encode error: {}", e)))
				);
			}

			let any_msg = ProtobufAny {
				type_url: "/cosmos.staking.v1beta1.MsgUndelegate".to_string(),
				value: Binary::from(buf),
			};

			let cosmos_msg = NeutronMsg::submit_tx(
				connection_id.clone(),
				interchain_account_id.clone(),
				vec![any_msg],
				"".to_string(),
				DEFAULT_TIMEOUT_SECONDS,
				fee.clone(),
			);

			// We use a submessage here because we need the process message reply to save
			// the outgoing IBC packet identifier for later.
			let submsg_unstake = msg_with_sudo_callback(
				deps.branch(),
				cosmos_msg,
				SudoPayload::HandlerPayloadInterTx(InterTxType {
					port_id: get_port_id(env.contract.address.to_string(), interchain_account_id),
					// Here you can store some information about the transaction to help you parse
					// the acknowledgement later.
					message: "interchain_undelegate".to_string(),
				}),
			)?;

			msgs.push(submsg_unstake);
		} else if pool_info.active - pool_info.need_withdraw > Int256::from(0) {
			let stake_amount = pool_info.active - pool_info.need_withdraw;
			// add submessage to stake
			let delegate_msg = MsgDelegate {
				delegator_address: pool_addr.clone(),
				validator_address: pool_info.validator_addrs[0].clone(),
				amount: Some(Coin {
					denom: pool_info.ibc_denom.clone(),
					amount: stake_amount.to_string(),
				}),
			};

			// Serialize the Delegate message.
			let mut buf = Vec::new();
			buf.reserve(delegate_msg.encoded_len());

			if let Err(e) = delegate_msg.encode(&mut buf) {
				return Err(
					NeutronError::Std(StdError::generic_err(format!("Encode error: {}", e)))
				);
			}

			// Put the serialized Delegate message to a types.Any protobuf message.
			let any_msg = ProtobufAny {
				type_url: "/cosmos.staking.v1beta1.MsgDelegate".to_string(),
				value: Binary::from(buf),
			};

			// Form the neutron SubmitTx message containing the binary Delegate message.
			let cosmos_msg = NeutronMsg::submit_tx(
				connection_id.clone(),
				interchain_account_id.clone(),
				vec![any_msg],
				"".to_string(),
				DEFAULT_TIMEOUT_SECONDS,
				fee.clone(),
			);

			// We use a submessage here because we need the process message reply to save
			// the outgoing IBC packet identifier for later.
			let submsg_stake = msg_with_sudo_callback(
				deps.branch(),
				cosmos_msg,
				SudoPayload::HandlerPayloadInterTx(InterTxType {
					port_id: get_port_id(env.contract.address.to_string(), interchain_account_id),
					// Here you can store some information about the transaction to help you parse
					// the acknowledgement later.
					message: "interchain_delegate".to_string(),
				}),
			)?;
			msgs.push(submsg_stake);
		}
	}

	Ok(Response::default().add_submessages(msgs))
}

fn execute_bond_active(
	mut deps: DepsMut<NeutronQuery>,
	_: Env,
	pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
	let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
	// todo: check era state
	// todo: calculate the rate
	// todo: calculate protocol fee
	Ok(Response::default())
}

// todo: add ibc tx result reply
// todo: update the rate in sudo replay
// todo: update era state
// todo: ica rewards
#[entry_point]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> StdResult<Response> {
	deps.api.debug(format!("WASMDEBUG: sudo: received sudo msg: {:?}", msg).as_str());

	match msg {
		// For handling successful (non-error) acknowledgements
		SudoMsg::Response { request, data } => sudo_response(deps, request, data),

		// For handling error acknowledgements
		SudoMsg::Error { request, details } => sudo_error(deps, request, details),

		// For handling error timeouts
		SudoMsg::Timeout { request } => sudo_timeout(deps, request),

		// For handling successful registering of ICA
		SudoMsg::OpenAck { port_id, channel_id, counterparty_channel_id, counterparty_version } =>
			sudo_open_ack(
				deps,
				env,
				port_id,
				channel_id,
				counterparty_channel_id,
				counterparty_version,
			),
		_ => Ok(Response::default()),
	}
}

fn sudo_error(deps: DepsMut, req: RequestPacket, data: String) -> StdResult<Response> {
	deps.api.debug(
		format!("WASMDEBUG: sudo_error: sudo error received: {:?} {}", req, data).as_str()
	);
	Ok(Response::new())
}

fn sudo_timeout(deps: DepsMut, req: RequestPacket) -> StdResult<Response> {
	deps.api.debug(
		format!("WASMDEBUG: sudo_timeout: sudo timeout ack received: {:?}", req).as_str()
	);
	Ok(Response::new())
}

fn sudo_response(deps: DepsMut, req: RequestPacket, data: Binary) -> StdResult<Response> {
	deps.api.debug(format!("WASMDEBUG: sudo_response: sudo received: {:?} {}", req, data).as_str());
	let seq_id = req.sequence.ok_or_else(|| StdError::generic_err("sequence not found"))?;
	let channel_id = req.source_channel.ok_or_else(||
		StdError::generic_err("channel_id not found")
	)?;

	match read_sudo_payload(deps.storage, channel_id, seq_id)? {
		SudoPayload::HandlerPayloadIbcSend(t) => sudo_ibc_send_callback(deps.as_ref(), t),
		SudoPayload::HandlerPayloadInterTx(t) => sudo_inter_tx_callback(deps.as_ref(), t),
	}
	// at this place we can safely remove the data under (channel_id, seq_id) key
	// but it costs an extra gas, so its on you how to use the storage
}

// handler
fn sudo_open_ack(
	deps: DepsMut,
	_env: Env,
	port_id: String,
	_channel_id: String,
	_counterparty_channel_id: String,
	counterparty_version: String,
) -> StdResult<Response> {
	// The version variable contains a JSON value with multiple fields,
	// including the generated account address.
	let parsed_version: Result<OpenAckVersion, _> = serde_json_wasm::from_str(
		counterparty_version.as_str()
	);

	// Update the storage record associated with the interchain account.
	if let Ok(parsed_version) = parsed_version {
		INTERCHAIN_ACCOUNTS.save(
			deps.storage,
			port_id,
			&Some((parsed_version.address.clone(), parsed_version.controller_connection_id.clone())),
		)?;
		POOL_ICA_MAP.save(
			deps.storage,
			parsed_version.address,
			&parsed_version.controller_connection_id,
		)?;
		return Ok(Response::default());
	}
	Err(StdError::generic_err("Can't parse counterparty_version"))
}

#[entry_point]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
	deps.api.debug("WASMDEBUG: migrate");
	Ok(Response::default())
}

fn min_ntrn_ibc_fee(fee: IbcFee) -> IbcFee {
	IbcFee {
		recv_fee: fee.recv_fee,
		ack_fee: fee.ack_fee
			.into_iter()
			.filter(|a| a.denom == FEE_DENOM)
			.collect(),
		timeout_fee: fee.timeout_fee
			.into_iter()
			.filter(|a| a.denom == FEE_DENOM)
			.collect(),
	}
}

fn get_ica(
	deps: Deps<impl CustomQuery>,
	env: &Env,
	interchain_account_id: &str,
) -> Result<(String, String), StdError> {
	let key = get_port_id(env.contract.address.as_str(), interchain_account_id);

	INTERCHAIN_ACCOUNTS.load(deps.storage, key)?.ok_or_else(||
		StdError::generic_err("Interchain account is not created yet")
	)
}

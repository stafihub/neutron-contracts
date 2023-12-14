use std::ops::{Add, Mul};
use cosmwasm_std::{coin, to_json_binary, entry_point, from_json, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError, StdResult, SubMsg, Uint128, Coin, WasmMsg, CustomQuery, Addr};
use cw2::set_contract_version;
use neutron_sdk::{bindings::{
	msg::{IbcFee, MsgIbcTransferResponse, NeutronMsg},
	query::NeutronQuery,
}, query::min_ibc_fee::query_min_ibc_fee, sudo::msg::{RequestPacket, RequestPacketTimeoutHeight, TransferSudoMsg}, NeutronResult, NeutronError};
use neutron_sdk::interchain_txs::helpers::get_port_id;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cw20_ratom::{ContractError, msg};
use crate::{
	msg::{ExecuteMsg, InstantiateMsg, MigrateMsg},
	state::{
		read_reply_payload, read_sudo_payload, save_reply_payload, save_sudo_payload,
		IBC_SUDO_ID_RANGE_END, IBC_SUDO_ID_RANGE_START,
	},
};
use crate::state::{INTERCHAIN_ACCOUNTS, POOL_INFOS, STATE, State, UnstakeInfo, UNSTAKES_INDEX_FOR_USER, UNSTAKES_OF_INDEX};

// Default timeout for IbcTransfer is 10000000 blocks
const DEFAULT_TIMEOUT_HEIGHT: u64 = 10000000;

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
	env: Env,
	info: MessageInfo,
	msg: InstantiateMsg,
) -> NeutronResult<Response<NeutronMsg>> {
	set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

	STATE.save(
		deps.storage, &(State {
			owner: info.sender,
			minimal_stake: msg.minimal_stake,
			cw20: msg.cw20_address,
			atom_ibc_denom: msg.atom_ibc_denom,
			era: Uint128::zero(),
			rate: Uint128::one(),
			unstake_times_limit: msg.unstake_times_limit,
			next_unstake_index: Uint128::zero(),
			unbonding_period: msg.unbonding_period,
		}),
	)?;

	let register = NeutronMsg::register_interchain_account(
		msg.connection_id,
		msg.interchain_account_id.clone(),
	);
	let key = get_port_id(env.contract.address.as_str(), &msg.interchain_account_id);
	// we are saving empty data here because we handle response of registering ICA in sudo_open_ack method
	INTERCHAIN_ACCOUNTS.save(deps.storage, key, &None)?;

	Ok(Response::new().add_message(register))
}

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
		ExecuteMsg::Stake {
			neutron_address
		} => execute_stake(deps, env, neutron_address, info),
		ExecuteMsg::Unstake {
			amount, interchain_account_id, receiver
		} => execute_unstake(deps, env, info, amount, interchain_account_id, receiver),
		ExecuteMsg::Withdraw {} => execute_withdraw(deps, env, info),
		ExecuteMsg::NewEra {
			channel, interchain_account_id
		} => execute_new_era(deps, env, info.funds, interchain_account_id, channel),
		ExecuteMsg::StakeLSM {} => execute_stake_lsm(deps, env, info),
	}
}

// Example of different payload types
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Type {
	pub message: String,
}

// a callback handler for payload of Type1
fn sudo_callback(deps: Deps, payload: Type) -> StdResult<Response> {
	deps.api
		.debug(format!("WASMDEBUG: callback: sudo payload: {:?}", payload).as_str());
	Ok(Response::new())
}

// todo: clean types and payload
// Enum representing payload to process during handling acknowledgement messages in Sudo handler
#[derive(Serialize, Deserialize)]
pub enum SudoPayload {
	HandlerPayload(Type),
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
			.data
			.ok_or_else(|| StdError::generic_err("no result"))?,
	)
		.map_err(|e| StdError::generic_err(format!("failed to parse response: {:?}", e)))?;
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
		_ => Err(StdError::generic_err(format!(
			"unsupported reply message id {}",
			msg.id
		))),
	}
}

fn execute_stake(
	deps: DepsMut<NeutronQuery>,
	_: Env,
	neutron_address: String,
	info: MessageInfo,
) -> NeutronResult<Response<NeutronMsg>> {
	let state = STATE.load(deps.storage)?;

	let mut amount = 0;
	if !info.funds.is_empty() {
		amount = u128::from(
			info.funds
				.iter()
				.find(|c| c.denom == state.atom_ibc_denom)
				.map(|c| c.amount)
				.unwrap_or(Uint128::zero())
		);
	}
	// todo: Exchange rate conversion in new era function
	amount = amount.mul(state.rate.u128());

	let msg = WasmMsg::Execute {
		contract_addr: state.cw20.to_string(),
		msg: to_json_binary(&msg::ExecuteMsg::Mint { recipient: neutron_address.to_string(), amount: Uint128::from(amount) })?,
		funds: vec![],
	};

	// todo: add event
	Ok(Response::new()
		.add_message(CosmosMsg::Wasm(msg))
		.add_attribute("mint", "call_contract_b"))
}

// todo: Don't consider the service charge for the time being.
// todo: Before this step, ask the user to authorize burn from
fn execute_unstake(
	deps: DepsMut<NeutronQuery>,
	env: Env,
	info: MessageInfo,
	amount: Uint128,
	interchain_account_id: String,
	receiver: Addr,
) -> NeutronResult<Response<NeutronMsg>> {
	if amount == Uint128::zero() {
		return Err(NeutronError::Std(StdError::generic_err(format!(
			"Encode error: {}", "LSD token amount is zero"
		))));
	}

	let mut state = STATE.load(deps.storage)?;

	// todo: Abstract as a method
	let unstake_count = UNSTAKES_INDEX_FOR_USER.load(deps.storage, &info.sender)?.len() as u128;
	let unstake_limit = state.unstake_times_limit.u128();
	if unstake_count >= unstake_limit {
		return Err(NeutronError::Std(StdError::generic_err(format!(
			"Encode error: {}", "Unstake times limit reached"
		))));
	}

	// Calculate the number of tokens(atom)
	let token_amount = amount.mul(state.rate);

	let (delegator, _) = get_ica(deps.as_ref(), &env, &interchain_account_id)?;

	// update pool info
	let mut pool_info = POOL_INFOS.load(deps.storage)?;
	pool_info.unbond += token_amount;
	pool_info.active -= token_amount;
	POOL_INFOS.save(deps.storage, &(pool_info))?;

	// update unstake info
	let unstake_info = UnstakeInfo {
		era: state.era,
		pool: delegator,
		receiver,
		amount: token_amount,
	};

	let will_use_unstake_index = state.next_unstake_index;

	UNSTAKES_OF_INDEX.save(deps.storage, will_use_unstake_index.u128(), &unstake_info)?;

	state.next_unstake_index = state.next_unstake_index.add(Uint128::one());
	STATE.save(deps.storage, &state)?;

	// burn
	let msg = WasmMsg::Execute {
		contract_addr: state.cw20.to_string(),
		msg: to_json_binary(&msg::ExecuteMsg::BurnFrom { owner: info.sender.to_string(), amount: Default::default() })?,
		funds: vec![],
	};

	// send event
	Ok(Response::new()
		.add_message(CosmosMsg::Wasm(msg))
		.add_attribute("action", "unstake")
		.add_attribute("from", info.sender)
		.add_attribute("token_amount", token_amount.to_string())
		.add_attribute("lsd_token_amount", amount.to_string())
		.add_attribute("unstake_index", will_use_unstake_index.to_string()))
}

fn execute_withdraw(
	deps: DepsMut<NeutronQuery>,
	_: Env,
	info: MessageInfo,
) -> NeutronResult<Response<NeutronMsg>> {

	Ok(Response::new())
}

fn execute_stake_lsm(
	_: DepsMut<NeutronQuery>,
	_: Env,
	_: MessageInfo,
) -> NeutronResult<Response<NeutronMsg>> {
	// todo!
	Ok(Response::new())
}

fn execute_new_era(
	mut deps: DepsMut<NeutronQuery>,
	env: Env,
	funds: Vec<Coin>,
	interchain_account_id: String,
	channel: String,
) -> NeutronResult<Response<NeutronMsg>> {
	// --------------------------------------------------------------------------------------------------
	// contract must pay for relaying of acknowledgements
	// See more info here: https://docs.neutron.org/neutron/feerefunder/overview
	let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
	let (delegator, _) = get_ica(deps.as_ref(), &env, &interchain_account_id)?;
	// --------------------------------------------------------------------------------------------------

	// todo: Funds is obtained from the internal status of the contract

	let state = STATE.load(deps.storage)?;

	let mut amount = 0;
	if !funds.is_empty() {
		amount = u128::from(
			funds
				.iter()
				.find(|c| c.denom == state.atom_ibc_denom)
				.map(|c| c.amount)
				.unwrap_or(Uint128::zero())
		);
	}

	let tx_coin = coin(amount, state.atom_ibc_denom);

	let msg = NeutronMsg::IbcTransfer {
		source_port: "transfer".to_string(),
		source_channel: channel.clone(),
		sender: env.contract.address.to_string(),
		receiver: delegator.clone(),
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
		SudoPayload::HandlerPayload(Type {
			message: "message".to_string(),
		}),
	)?;

	deps.as_ref().api.debug(format!("WASMDEBUG: execute_send: sent submsg: {:?}", submsg).as_str());

	Ok(Response::default().add_submessages(vec![submsg]))
}

#[entry_point]
pub fn sudo(deps: DepsMut, _env: Env, msg: TransferSudoMsg) -> StdResult<Response> {
	match msg {
		// For handling successful (non-error) acknowledgements
		TransferSudoMsg::Response { request, data } => sudo_response(deps, request, data),

		// For handling error acknowledgements
		TransferSudoMsg::Error { request, details } => sudo_error(deps, request, details),

		// For handling error timeouts
		TransferSudoMsg::Timeout { request } => sudo_timeout(deps, request),
	}
}

fn sudo_error(deps: DepsMut, req: RequestPacket, data: String) -> StdResult<Response> {
	deps.api.debug(
		format!(
			"WASMDEBUG: sudo_error: sudo error received: {:?} {}",
			req, data
		)
			.as_str(),
	);
	Ok(Response::new())
}

fn sudo_timeout(deps: DepsMut, req: RequestPacket) -> StdResult<Response> {
	deps.api.debug(
		format!(
			"WASMDEBUG: sudo_timeout: sudo timeout ack received: {:?}",
			req
		)
			.as_str(),
	);
	Ok(Response::new())
}

fn sudo_response(deps: DepsMut, req: RequestPacket, data: Binary) -> StdResult<Response> {
	deps.api.debug(
		format!(
			"WASMDEBUG: sudo_response: sudo received: {:?} {}",
			req, data
		)
			.as_str(),
	);
	let seq_id = req
		.sequence
		.ok_or_else(|| StdError::generic_err("sequence not found"))?;
	let channel_id = req
		.source_channel
		.ok_or_else(|| StdError::generic_err("channel_id not found"))?;

	match read_sudo_payload(deps.storage, channel_id, seq_id)? {
		SudoPayload::HandlerPayload(t) => sudo_callback(deps.as_ref(), t),
	}
	// at this place we can safely remove the data under (channel_id, seq_id) key
	// but it costs an extra gas, so its on you how to use the storage
}

#[entry_point]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
	deps.api.debug("WASMDEBUG: migrate");
	Ok(Response::default())
}

fn min_ntrn_ibc_fee(fee: IbcFee) -> IbcFee {
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
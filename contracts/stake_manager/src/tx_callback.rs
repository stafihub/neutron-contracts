use cosmwasm_std::{
    from_json, Binary, CosmosMsg, DepsMut, Env, Reply, Response, StdError, StdResult, SubMsg,
};

use neutron_sdk::bindings::{msg::MsgIbcTransferResponse, query::NeutronQuery};
use neutron_sdk::sudo::msg::RequestPacket;

use crate::execute_era_restake::sudo_era_restake_callback;
use crate::execute_era_restake::sudo_era_restake_failed_callback;
use crate::execute_pool_update_validator::{
    sudo_update_validators_callback, sudo_update_validators_failed_callback,
};
use crate::execute_redeem_token_for_share::sudo_redeem_token_for_share_callback;
use crate::execute_stake_lsm::{sudo_stake_lsm_callback, sudo_stake_lsm_failed_callback};
use crate::execute_withdraw::{sudo_withdraw_callback, sudo_withdraw_failed_callback};
use crate::state::{
    read_reply_payload, read_sudo_payload, save_reply_payload, save_sudo_payload, SudoPayload,
    TxType,
};
use crate::{
    execute_era_bond::sudo_era_bond_callback,
    execute_era_bond::sudo_era_bond_failed_callback,
    execute_era_collect_withdraw::{
        sudo_era_collect_withdraw_callback, sudo_era_collect_withdraw_failed_callback,
    },
    execute_era_update::sudo_era_update_callback,
    execute_era_update::sudo_era_update_failed_callback,
};

// Default timeout for IbcTransfer is 10000000 blocks
pub const DEFAULT_TIMEOUT_HEIGHT: u64 = 10000000;

// Default timeout for SubmitTX is two weeks
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 60 * 60 * 24 * 7 * 2;

pub const DEFAULT_UPDATE_PERIOD: u64 = 6;

// saves payload to process later to the storage and returns a SubmitTX Cosmos SubMsg with necessary reply id
pub fn msg_with_sudo_callback<C: Into<CosmosMsg<T>>, T>(
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
pub fn prepare_sudo_payload(mut deps: DepsMut, _env: Env, msg: Reply) -> StdResult<Response> {
    let payload = read_reply_payload(deps.storage, msg.id)?;

    let resp: MsgIbcTransferResponse = from_json(
        msg.result
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

pub fn sudo_response(
    deps: DepsMut,
    env: Env,
    req: RequestPacket,
    data: Binary,
) -> StdResult<Response> {
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

    if let Ok(payload) = read_sudo_payload(deps.storage, channel_id, seq_id) {
        return sudo_callback(deps, env, payload);
    }

    Err(StdError::generic_err("Error message"))
    // at this place we can safely remove the data under (channel_id, seq_id) key
    // but it costs an extra gas, so its on you how to use the storage
}

pub fn sudo_error(deps: DepsMut, req: RequestPacket, data: String) -> StdResult<Response> {
    deps.api.debug(
        format!(
            "WASMDEBUG: sudo_error: sudo error received: {:?} {}",
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

    if let Ok(payload) = read_sudo_payload(deps.storage, channel_id, seq_id) {
        return sudo_failed_callback(deps, payload);
    }

    Ok(Response::new())
}

pub fn sudo_timeout(deps: DepsMut, req: RequestPacket) -> StdResult<Response> {
    deps.api.debug(
        format!(
            "WASMDEBUG: sudo_timeout: sudo timeout ack received: {:?}",
            req
        )
        .as_str(),
    );

    let seq_id = req
        .sequence
        .ok_or_else(|| StdError::generic_err("sequence not found"))?;
    let channel_id = req
        .source_channel
        .ok_or_else(|| StdError::generic_err("channel_id not found"))?;

    if let Ok(payload) = read_sudo_payload(deps.storage, channel_id, seq_id) {
        return sudo_failed_callback(deps, payload);
    }

    Ok(Response::new())
}

fn sudo_callback(deps: DepsMut, env: Env, payload: SudoPayload) -> StdResult<Response> {
    match payload.tx_type {
        TxType::EraUpdate => sudo_era_update_callback(deps, payload),
        TxType::EraBond => sudo_era_bond_callback(deps, env, payload),
        TxType::EraCollectWithdraw => sudo_era_collect_withdraw_callback(deps, env, payload),
        TxType::EraRestake => sudo_era_restake_callback(deps, env, payload),
        TxType::UserWithdraw => sudo_withdraw_callback(deps, payload),
        TxType::UpdateValidators => sudo_update_validators_callback(deps, payload),
        TxType::StakeLsm => sudo_stake_lsm_callback(deps, payload),
        TxType::RedeemTokenForShare => sudo_redeem_token_for_share_callback(deps, payload),

        _ => Ok(Response::new()),
    }
}

fn sudo_failed_callback(deps: DepsMut, payload: SudoPayload) -> StdResult<Response> {
    match payload.tx_type {
        TxType::EraUpdate => sudo_era_update_failed_callback(deps, payload),
        TxType::EraBond => sudo_era_bond_failed_callback(deps, payload),
        TxType::EraCollectWithdraw => sudo_era_collect_withdraw_failed_callback(deps, payload),
        TxType::EraRestake => sudo_era_restake_failed_callback(deps, payload),
        TxType::UserWithdraw => sudo_withdraw_failed_callback(deps, payload),
        TxType::UpdateValidators => sudo_update_validators_failed_callback(deps, payload),
        TxType::StakeLsm => sudo_stake_lsm_failed_callback(payload),

        _ => Ok(Response::new()),
    }
}

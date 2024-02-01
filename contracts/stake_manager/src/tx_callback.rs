use crate::execute_era_rebond::sudo_era_rebond_failed_callback;
use crate::execute_pool_update_validator::{
    sudo_update_validator_callback, sudo_update_validator_failed_callback,
};
use crate::execute_redeem_token_for_share::{
    sudo_redeem_token_for_share_callback, sudo_redeem_token_for_share_failed_callback,
};
use crate::execute_stake_lsm::{sudo_stake_lsm_callback, sudo_stake_lsm_failed_callback};
use crate::execute_withdraw::{sudo_withdraw_callback, sudo_withdraw_failed_callback};
use crate::helper::sudo_set_withdraw_addr_failed_callback;
use crate::state::{
    read_reply_payload, read_sudo_payload, save_reply_payload, save_sudo_payload, SudoPayload,
    TxType, SUDO_PAYLOAD,
};
use crate::{error_conversion::ContractError, execute_era_rebond::sudo_era_rebond_callback};
use crate::{
    execute_era_bond::sudo_era_bond_callback,
    execute_era_bond::sudo_era_bond_failed_callback,
    execute_era_collect_withdraw::{
        sudo_era_collect_withdraw_callback, sudo_era_collect_withdraw_failed_callback,
    },
    execute_era_update::sudo_era_update_callback,
    execute_era_update::sudo_era_update_failed_callback,
};
use crate::{
    execute_pool_rm_validator::{sudo_rm_validator_callback, sudo_rm_validator_failed_callback},
    helper::sudo_set_withdraw_addr_callback,
};
use cosmwasm_std::{
    from_json, Binary, CosmosMsg, DepsMut, Env, Reply, Response, StdError, StdResult, SubMsg,
};
use neutron_sdk::sudo::msg::RequestPacket;
use neutron_sdk::{
    bindings::{
        msg::{MsgIbcTransferResponse, NeutronMsg},
        query::NeutronQuery,
    },
    NeutronResult,
};

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
            .ok_or_else(|| ContractError::ICQErrReplyNoResult {})?,
    )
    .map_err(|e| ContractError::ICQErrFailedParse(e.to_string()))?;

    let seq_id = resp.sequence_id;
    let channel_id = resp.channel;
    save_sudo_payload(deps.branch().storage, channel_id, seq_id, payload)?;
    Ok(Response::new())
}

pub fn sudo_response(
    deps: DepsMut,
    env: Env,
    req: RequestPacket,
    _: Binary,
) -> NeutronResult<Response<NeutronMsg>> {
    let seq_id = req
        .sequence
        .ok_or_else(|| ContractError::CallBackErrSequenceNotFound {})?;
    let channel_id = req
        .source_channel
        .ok_or_else(|| ContractError::CallBackErrChannelIDNotFound {})?;

    if let Ok(payload) = read_sudo_payload(deps.storage, channel_id.clone(), seq_id) {
        SUDO_PAYLOAD.remove(deps.storage, (channel_id, seq_id));
        return sudo_callback(deps, env, payload);
    }

    Err(ContractError::CallBackErrErrorMsg {}.into())
}

pub fn sudo_error(
    deps: DepsMut,
    req: RequestPacket,
    _: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let seq_id = req
        .sequence
        .ok_or_else(|| ContractError::CallBackErrSequenceNotFound {})?;
    let channel_id = req
        .source_channel
        .ok_or_else(|| ContractError::CallBackErrChannelIDNotFound {})?;

    if let Ok(payload) = read_sudo_payload(deps.storage, channel_id.clone(), seq_id) {
        SUDO_PAYLOAD.remove(deps.storage, (channel_id, seq_id));
        return sudo_failed_callback(deps, payload);
    }

    Err(ContractError::CallBackErrErrorMsg {}.into())
}

pub fn sudo_timeout(deps: DepsMut, req: RequestPacket) -> NeutronResult<Response<NeutronMsg>> {
    let seq_id = req
        .sequence
        .ok_or_else(|| ContractError::CallBackErrSequenceNotFound {})?;
    let channel_id = req
        .source_channel
        .ok_or_else(|| ContractError::CallBackErrChannelIDNotFound {})?;

    if let Ok(payload) = read_sudo_payload(deps.storage, channel_id.clone(), seq_id) {
        SUDO_PAYLOAD.remove(deps.storage, (channel_id, seq_id));
        return sudo_failed_callback(deps, payload);
    }

    Err(ContractError::CallBackErrErrorMsg {}.into())
}

fn sudo_callback(
    deps: DepsMut,
    env: Env,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    match payload.tx_type {
        TxType::SetWithdrawAddr => sudo_set_withdraw_addr_callback(deps, payload),
        TxType::EraUpdate => sudo_era_update_callback(deps, env, payload),
        TxType::EraBond => sudo_era_bond_callback(deps, env, payload),
        TxType::EraCollectWithdraw => sudo_era_collect_withdraw_callback(deps, env, payload),
        TxType::EraRebond => sudo_era_rebond_callback(deps, env, payload),
        TxType::UserWithdraw => sudo_withdraw_callback(deps, payload),
        TxType::UpdateValidator => sudo_update_validator_callback(deps, payload),
        TxType::RmValidator => sudo_rm_validator_callback(deps, payload),
        TxType::StakeLsm => sudo_stake_lsm_callback(deps, payload),
        TxType::RedeemTokenForShare => sudo_redeem_token_for_share_callback(deps, payload),
    }
}

fn sudo_failed_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    match payload.tx_type {
        TxType::SetWithdrawAddr => sudo_set_withdraw_addr_failed_callback(deps, payload),
        TxType::EraUpdate => sudo_era_update_failed_callback(deps, payload),
        TxType::EraBond => sudo_era_bond_failed_callback(deps, payload),
        TxType::EraCollectWithdraw => sudo_era_collect_withdraw_failed_callback(deps, payload),
        TxType::EraRebond => sudo_era_rebond_failed_callback(deps, payload),
        TxType::UserWithdraw => sudo_withdraw_failed_callback(deps, payload),
        TxType::UpdateValidator => sudo_update_validator_failed_callback(deps, payload),
        TxType::RmValidator => sudo_rm_validator_failed_callback(deps, payload),
        TxType::StakeLsm => sudo_stake_lsm_failed_callback(deps, payload),
        TxType::RedeemTokenForShare => sudo_redeem_token_for_share_failed_callback(deps, payload),
    }
}

use cosmwasm_std::{DepsMut, Reply, Response, StdError, StdResult};

use neutron_sdk::bindings::msg::MsgRegisterInterchainQueryResponse;

use crate::state::{
    KV_QUERY_ID_TO_CALLBACKS,
    REPLY_ID_TO_QUERY_ID, QueryKind,
};

// save query_id to query_type information in reply, so that we can understand the kind of query we're getting in sudo kv call
pub fn write_balance_query_id_to_reply_id(deps: DepsMut, msg: Reply) -> StdResult<Response> {
    let resp: MsgRegisterInterchainQueryResponse = serde_json_wasm::from_slice(
        msg.result
            .into_result()
            .map_err(StdError::generic_err)?
            .data
            .ok_or_else(|| StdError::generic_err("no result"))?
            .as_slice(),
    )
    .map_err(|e| StdError::generic_err(format!("failed to parse response: {:?}", e)))?;

    deps.api.debug(
        format!(
            "WASMDEBUG: write_balance_query_id_to_reply_id query_id: {:?} msg.id(Reply id): {:?}",
            resp.id, msg.id
        )
        .as_str(),
    );

    KV_QUERY_ID_TO_CALLBACKS.save(deps.storage, resp.id, &QueryKind::Balances)?;
    REPLY_ID_TO_QUERY_ID.save(deps.storage, msg.id, &resp.id)?;

    Ok(Response::default())
}

pub fn write_delegation_query_id_to_reply_id(deps: DepsMut, msg: Reply) -> StdResult<Response> {
    let resp: MsgRegisterInterchainQueryResponse = serde_json_wasm::from_slice(
        msg.result
            .into_result()
            .map_err(StdError::generic_err)?
            .data
            .ok_or_else(|| StdError::generic_err("no result"))?
            .as_slice(),
    )
    .map_err(|e| StdError::generic_err(format!("failed to parse query response: {:?}", e)))?;

    deps.api.debug(
        format!(
            "WASMDEBUG: write_delegation_query_id_to_reply_id query_id: {:?} msg.id(Reply id): {:?}",
            resp.id, msg.id
        )
        .as_str(),
    );

    KV_QUERY_ID_TO_CALLBACKS.save(deps.storage, resp.id, &QueryKind::Delegations)?;
    REPLY_ID_TO_QUERY_ID.save(deps.storage, msg.id, &resp.id)?;

    Ok(Response::default())
}

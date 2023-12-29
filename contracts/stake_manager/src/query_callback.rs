use cosmwasm_std::{DepsMut, Reply, Response, StdError, StdResult};
use neutron_sdk::bindings::msg::MsgRegisterInterchainQueryResponse;

use crate::state::{QueryKind, KV_QUERY_ID_TO_CALLBACKS, OWN_QUERY_ID_TO_ICQ_ID};

// save query_id to query_type information in reply, so that we can understand the kind of query we're getting in sudo kv call
pub fn write_balance_query_id_to_reply_id(deps: DepsMut, reply: Reply) -> StdResult<Response> {
    let resp: MsgRegisterInterchainQueryResponse = serde_json_wasm::from_slice(
        reply
            .result
            .into_result()
            .map_err(StdError::generic_err)?
            .data
            .ok_or_else(|| StdError::generic_err("no result"))?
            .as_slice(),
    )
    .map_err(|e| StdError::generic_err(format!("failed to parse response: {:?}", e)))?;

    deps.api.debug(
        format!(
            "WASMDEBUG: write_balance_query_id_to_reply_id query_id: {:?}",
            resp.id
        )
        .as_str(),
    );

    KV_QUERY_ID_TO_CALLBACKS.save(deps.storage, resp.id, &QueryKind::Balances)?;
    OWN_QUERY_ID_TO_ICQ_ID.save(deps.storage, reply.id, &resp.id)?;

    Ok(Response::default())
}

pub fn write_delegation_query_id_to_reply_id(deps: DepsMut, reply: Reply) -> StdResult<Response> {
    let resp: MsgRegisterInterchainQueryResponse = serde_json_wasm::from_slice(
        reply
            .result
            .into_result()
            .map_err(StdError::generic_err)?
            .data
            .ok_or_else(|| StdError::generic_err("no result"))?
            .as_slice(),
    )
    .map_err(|e| StdError::generic_err(format!("failed to parse query response: {:?}", e)))?;

    deps.api.debug(
        format!(
            "WASMDEBUG: write_delegation_query_id_to_reply_id query_id: {:?}",
            resp.id
        )
        .as_str(),
    );

    KV_QUERY_ID_TO_CALLBACKS.save(deps.storage, resp.id, &QueryKind::Delegations)?;
    OWN_QUERY_ID_TO_ICQ_ID.save(deps.storage, reply.id, &resp.id)?;

    Ok(Response::default())
}

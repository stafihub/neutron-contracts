use cosmwasm_std::{CosmosMsg, DepsMut, Reply, Response, StdError, StdResult, SubMsg};

use neutron_sdk::bindings::{msg::MsgRegisterInterchainQueryResponse, query::NeutronQuery};

use crate::{
    error_conversion::ContractError,
    state::{get_next_query_reply_id, QueryKind, ADDRESS_TO_REPLY_ID, REPLY_ID_TO_QUERY_ID},
};

pub fn register_query_submsg<C: Into<CosmosMsg<T>>, T>(
    deps: DepsMut<NeutronQuery>,
    msg: C,
    addr: String,
    query_kind: QueryKind,
) -> StdResult<SubMsg<T>> {
    let next_reply_id = get_next_query_reply_id(deps.storage)?;

    ADDRESS_TO_REPLY_ID.save(deps.storage, (addr, query_kind.to_string()), &next_reply_id)?;

    Ok(SubMsg::reply_on_success(msg, next_reply_id))
}

// save query_id to query_type information in reply, so that we can understand the kind of query we're getting in sudo kv call
pub fn write_reply_id_to_query_id(deps: DepsMut, msg: Reply) -> StdResult<Response> {
    let resp: MsgRegisterInterchainQueryResponse = serde_json_wasm::from_slice(
        msg.result
            .into_result()
            .map_err(StdError::generic_err)?
            .data
            .ok_or_else(|| ContractError::ICQErrReplyNoResult {})?
            .as_slice(),
    )
    .map_err(|e| ContractError::ICQErrFailedParse(e.to_string()))?;

    deps.api.debug(
        format!(
            "WASMDEBUG: write_query_id_to_reply_id query_id: {:?} msg.id(Reply id): {:?}",
            resp.id, msg.id
        )
        .as_str(),
    );

    REPLY_ID_TO_QUERY_ID.save(deps.storage, msg.id, &resp.id)?;

    Ok(Response::default())
}

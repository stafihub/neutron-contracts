use crate::helper::DEFAULT_UPDATE_PERIOD;
use crate::state::{QUERY_ID_TO_REPLY_ID, REPLY_ID_TO_NEED_UPDATE};
use crate::{
    error_conversion::ContractError,
    state::{get_next_query_reply_id, QueryKind, ADDRESS_TO_REPLY_ID, REPLY_ID_TO_QUERY_ID},
};
use cosmwasm_std::{CosmosMsg, DepsMut, Reply, Response, StdError, StdResult, SubMsg};
use neutron_sdk::bindings::msg::NeutronMsg;
use neutron_sdk::bindings::{msg::MsgRegisterInterchainQueryResponse, query::NeutronQuery};
use neutron_sdk::NeutronResult;

pub fn register_query_submsg<C: Into<CosmosMsg<T>>, T>(
    deps: DepsMut<NeutronQuery>,
    msg: C,
    addr: String,
    query_kind: QueryKind,
) -> StdResult<SubMsg<T>> {
    let next_reply_id = get_next_query_reply_id(deps.storage)?;

    ADDRESS_TO_REPLY_ID.save(
        deps.storage,
        (addr.clone(), query_kind.clone().to_string()),
        &next_reply_id,
    )?;
    if query_kind == QueryKind::Validators {
        REPLY_ID_TO_NEED_UPDATE.save(deps.storage, next_reply_id, &true)?;
    }

    Ok(SubMsg::reply_on_success(msg, next_reply_id))
}

// save query_id to query_type information in reply, so that we can understand the kind of query we're getting in sudo kv call
pub fn write_reply_id_to_query_id(deps: DepsMut, msg: Reply) -> StdResult<Response> {
    let resp: MsgRegisterInterchainQueryResponse = serde_json_wasm::from_slice(
        msg.result
            .into_result()
            .map_err(StdError::generic_err)?
            .data
            .ok_or(ContractError::ICQErrReplyNoResult {})?
            .as_slice(),
    )
    .map_err(|e| ContractError::ICQErrFailedParse(e.to_string()))?;

    REPLY_ID_TO_QUERY_ID.save(deps.storage, msg.id, &resp.id)?;
    QUERY_ID_TO_REPLY_ID.save(deps.storage, resp.id, &msg.id)?;

    Ok(Response::default())
}

pub fn sudo_kv_query_result(deps: DepsMut, query_id: u64) -> NeutronResult<Response<NeutronMsg>> {
    let reply_id_result = QUERY_ID_TO_REPLY_ID.may_load(deps.storage, query_id)?;

    if let Some(reply_id) = reply_id_result {
        QUERY_ID_TO_REPLY_ID.remove(deps.storage, query_id);
        let need_update_status = REPLY_ID_TO_NEED_UPDATE
            .may_load(deps.storage, reply_id)
            .unwrap_or(Some(false));

        if need_update_status.unwrap_or(false) {
            REPLY_ID_TO_NEED_UPDATE.remove(deps.storage, reply_id);

            let update_msg = NeutronMsg::update_interchain_query(
                query_id,
                None,
                Some(DEFAULT_UPDATE_PERIOD),
                None,
            )?;

            return Ok(Response::new().add_message(update_msg));
        }
    }

    Ok(Response::new())
}

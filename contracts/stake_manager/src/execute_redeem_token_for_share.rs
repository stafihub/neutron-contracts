use cosmwasm_std::{DepsMut, MessageInfo, Response, StdError, StdResult};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronError, NeutronResult,
};

use crate::{
    contract::DEFAULT_TIMEOUT_SECONDS,
    helper::{min_ntrn_ibc_fee, redeem_token_for_share_msg},
    state::POOLS,
};
use crate::{
    state::{SudoPayload, TxType, INFO_OF_ICA_ID},
    tx_callback::msg_with_sudo_callback,
};

pub fn execute_redeem_token_for_share(
    mut deps: DepsMut<NeutronQuery>,
    _: MessageInfo,
    pool_addr: String,
    tokens: Vec<cosmwasm_std::Coin>,
) -> NeutronResult<Response<NeutronMsg>> {
    if tokens.len() == 0 || tokens.len() > 10 {
        return Err(NeutronError::Std(StdError::generic_err(
            "tokens len not match",
        )));
    }

    let pool_info = POOLS.load(deps.as_ref().storage, pool_addr.clone())?;
    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let denoms: Vec<String> = tokens.iter().map(|token| token.denom.clone()).collect();

    let submsg = msg_with_sudo_callback(
        deps.branch(),
        NeutronMsg::submit_tx(
            pool_ica_info.ctrl_connection_id,
            pool_info.ica_id.clone(),
            tokens
                .iter()
                .map(|token| {
                    redeem_token_for_share_msg(pool_ica_info.ica_addr.clone(), token.clone())
                })
                .collect(),
            "".to_string(),
            DEFAULT_TIMEOUT_SECONDS,
            fee,
        ),
        SudoPayload {
            port_id: pool_ica_info.ctrl_port_id,
            // the acknowledgement later
            message: denoms.join(","),
            pool_addr: pool_addr.clone(),
            tx_type: TxType::RedeemTokenForShare,
        },
    )?;

    Ok(Response::default().add_submessage(submsg))
}

pub fn sudo_redeem_token_for_share_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> StdResult<Response> {
    let mut pool_info = POOLS.load(deps.as_ref().storage, payload.pool_addr.clone())?;

    let will_removed_denoms: Vec<String> = payload.message.split(",").map(String::from).collect();

    pool_info
        .pending_share_tokens
        .retain(|token| !will_removed_denoms.contains(&token.denom));

    POOLS.save(deps.storage, payload.pool_addr, &pool_info)?;

    Ok(Response::new())
}

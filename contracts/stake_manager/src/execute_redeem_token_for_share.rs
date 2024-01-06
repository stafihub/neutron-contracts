use cosmwasm_std::{DepsMut, MessageInfo, Response};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronResult,
};

use crate::{
    contract::{msg_with_sudo_callback, SudoPayload, TxType, DEFAULT_TIMEOUT_SECONDS},
    helper::{min_ntrn_ibc_fee, redeem_token_for_share_msg},
    state::POOLS,
};
use crate::{error_conversion::ContractError, state::INFO_OF_ICA_ID};

pub fn execute_redeem_token_for_share(
    mut deps: DepsMut<NeutronQuery>,
    info: MessageInfo,
    pool_addr: String,
    tokens: Vec<cosmwasm_std::Coin>,
) -> NeutronResult<Response<NeutronMsg>> {
    let pool_info = POOLS.load(deps.as_ref().storage, pool_addr.clone())?;

    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }

    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

    let mut msgs = vec![];
    for token in tokens {
        msgs.push(redeem_token_for_share_msg(
            pool_ica_info.ica_addr.clone(),
            token,
        ));
    }

    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let cosmos_msg = NeutronMsg::submit_tx(
        pool_ica_info.ctrl_connection_id,
        pool_info.ica_id.clone(),
        msgs,
        "".to_string(),
        DEFAULT_TIMEOUT_SECONDS,
        fee,
    );

    let submsg = msg_with_sudo_callback(
        deps.branch(),
        cosmos_msg,
        SudoPayload {
            port_id: pool_ica_info.ctrl_port_id,
            // the acknowledgement later
            message: "".to_string(),
            pool_addr: pool_addr.clone(),
            tx_type: TxType::RedeemTokenForShare,
        },
    )?;

    Ok(Response::default().add_submessage(submsg))
}

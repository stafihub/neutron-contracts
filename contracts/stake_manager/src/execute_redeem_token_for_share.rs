use crate::{
    error_conversion::ContractError,
    helper::{check_ibc_fee, redeem_token_for_share_msg, DEFAULT_TIMEOUT_SECONDS},
    state::POOLS,
};
use crate::{
    state::{SudoPayload, TxType, INFO_OF_ICA_ID},
    tx_callback::msg_with_sudo_callback,
};
use cosmwasm_std::{DepsMut, MessageInfo, Response};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

pub fn execute_redeem_token_for_share(
    mut deps: DepsMut<NeutronQuery>,
    info: MessageInfo,
    pool_addr: String,
    tokens: Vec<cosmwasm_std::Coin>,
) -> NeutronResult<Response<NeutronMsg>> {
    if tokens.len() == 0 || tokens.len() > 10 {
        return Err(ContractError::TokensLenNotMatch {}.into());
    }
    let mut pool_info = POOLS.load(deps.as_ref().storage, pool_addr.clone())?;
    if !pool_info.lsm_support {
        return Err(ContractError::LsmStakeNotSupport {}.into());
    }

    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

    let mut denoms = vec![];
    let mut msgs = vec![];

    for token in &tokens {
        if !pool_info.share_tokens.contains(token) {
            return Err(ContractError::ShareTokenNotExist {}.into());
        }
        if denoms.contains(&token.denom) {
            return Err(ContractError::DuplicateToken {}.into());
        }

        denoms.push(token.denom.clone());
        pool_info
            .redeemming_share_token_denom
            .push(token.denom.clone());

        msgs.push(redeem_token_for_share_msg(
            pool_ica_info.ica_addr.clone(),
            token.clone(),
        ));
    }

    let ibc_fee = check_ibc_fee(deps.as_ref(), &info)?;
    let submsg = msg_with_sudo_callback(
        deps.branch(),
        NeutronMsg::submit_tx(
            pool_ica_info.ctrl_connection_id,
            pool_info.ica_id.clone(),
            msgs,
            "".to_string(),
            DEFAULT_TIMEOUT_SECONDS,
            ibc_fee,
        ),
        SudoPayload {
            port_id: pool_ica_info.ctrl_port_id,
            // the acknowledgement later
            message: denoms.join(","),
            pool_addr: pool_addr.clone(),
            tx_type: TxType::RedeemTokenForShare,
        },
    )?;

    POOLS.save(deps.storage, pool_addr, &pool_info)?;

    Ok(Response::default().add_submessage(submsg))
}

pub fn sudo_redeem_token_for_share_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.as_ref().storage, payload.pool_addr.clone())?;

    let will_removed_denoms: Vec<String> = payload.message.split(",").map(String::from).collect();

    pool_info
        .share_tokens
        .retain(|token| !will_removed_denoms.contains(&token.denom));

    pool_info
        .redeemming_share_token_denom
        .retain(|denom| !will_removed_denoms.contains(denom));

    POOLS.save(deps.storage, payload.pool_addr, &pool_info)?;

    Ok(Response::new())
}

pub fn sudo_redeem_token_for_share_failed_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.as_ref().storage, payload.pool_addr.clone())?;

    let will_removed_denoms: Vec<String> = payload.message.split(",").map(String::from).collect();

    pool_info
        .redeemming_share_token_denom
        .retain(|denom| !will_removed_denoms.contains(denom));

    POOLS.save(deps.storage, payload.pool_addr, &pool_info)?;

    Ok(Response::new())
}

use std::vec;

use cosmwasm_std::{DepsMut, Env, MessageInfo, Response};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

use crate::error_conversion::ContractError;
use crate::{
    helper::get_withdraw_ica_id,
    state::{INFO_OF_ICA_ID, POOLS},
};

pub fn execute_open_channel(
    deps: DepsMut<NeutronQuery>,
    _: Env,
    info: MessageInfo,
    pool_addr: String,
    closed_channel_id: String,
    register_fee: Vec<cosmwasm_std::Coin>,
) -> NeutronResult<Response<NeutronMsg>> {
    let pool_info = POOLS.load(deps.as_ref().storage, pool_addr)?;
    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }

    let mut msgs = vec![];

    let (pool_ica_info, withdraw_ica_info, _) =
        INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;
    if closed_channel_id.eq(&pool_ica_info.ctrl_channel_id.clone()) {
        let register_pool_msg = NeutronMsg::register_interchain_account(
            pool_ica_info.ctrl_connection_id.clone(),
            pool_info.ica_id.clone(),
            Some(register_fee.clone()),
        );
        msgs.push(register_pool_msg);
    } else if closed_channel_id.eq(&withdraw_ica_info.ctrl_channel_id.clone()) {
        let register_withdraw_msg = NeutronMsg::register_interchain_account(
            withdraw_ica_info.ctrl_connection_id.clone(),
            get_withdraw_ica_id(pool_info.ica_id),
            Some(register_fee.clone()),
        );
        msgs.push(register_withdraw_msg);
    } else {
        return Err(ContractError::ClosedChannelIdUnmatch {}.into());
    }

    Ok(Response::default().add_messages(msgs))
}

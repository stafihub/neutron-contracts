use cosmwasm_std::{DepsMut, Response, MessageInfo, StdError};

use neutron_sdk::{bindings::msg::NeutronMsg, NeutronError};
use neutron_sdk::bindings::query::NeutronQuery;
use neutron_sdk::NeutronResult;

use crate::state::{LSD_TOKEN_CODE_ID, STACK};

pub fn execute_update_lsd_token_code_id(
    deps: DepsMut<NeutronQuery>,
    info: MessageInfo,
    code_id: u64,
) -> NeutronResult<Response<NeutronMsg>> {
    let state = STACK.load(deps.storage)?;
    if !state.operators.contains(&info.sender) {
        return Err(NeutronError::Std(StdError::generic_err("Invalid operator")));
    }
    LSD_TOKEN_CODE_ID.save(deps.storage, &code_id)?;
    Ok(Response::default())
}

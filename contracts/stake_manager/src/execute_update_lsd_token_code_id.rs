use cosmwasm_std::{DepsMut, Response};

use neutron_sdk::bindings::msg::NeutronMsg;
use neutron_sdk::bindings::query::NeutronQuery;
use neutron_sdk::NeutronResult;

use crate::state::LSD_TOKEN_CODE_ID;

pub fn execute_update_lsd_token_code_id(
    deps: DepsMut<NeutronQuery>,
    code_id: u64,
) -> NeutronResult<Response<NeutronMsg>> {
    LSD_TOKEN_CODE_ID.save(deps.storage, &code_id)?;
    Ok(Response::default())
}

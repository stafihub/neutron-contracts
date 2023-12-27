use cosmwasm_std::{DepsMut, Env, MessageInfo, Response};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

// todo!
pub fn execute_stake_lsm(
    _: DepsMut<NeutronQuery>,
    _: Env,
    _: MessageInfo,
) -> NeutronResult<Response<NeutronMsg>> {
    Ok(Response::new())
}

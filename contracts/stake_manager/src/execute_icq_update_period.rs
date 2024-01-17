use cosmwasm_std::Response;
use neutron_sdk::{NeutronResult, bindings::msg::NeutronMsg};

pub fn update_icq_update_period(
    query_id: u64,
    new_update_period: u64,
) -> NeutronResult<Response<NeutronMsg>> {
    let update_msg =
        NeutronMsg::update_interchain_query(query_id, None, Some(new_update_period), None)?;
    Ok(Response::new().add_message(update_msg))
}
use cosmwasm_std::{DepsMut, MessageInfo, Response};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

use crate::state::STACK;
use crate::{error_conversion::ContractError, msg::ConfigStackParams};

pub fn execute_config_stack(
    deps: DepsMut<NeutronQuery>,
    info: MessageInfo,
    param: ConfigStackParams,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut stack = STACK.load(deps.storage)?;
    if stack.admin != info.sender {
        return Err(ContractError::Unauthorized {}.into());
    }
    if let Some(stack_fee_receiver) = param.stack_fee_receiver {
        stack.stack_fee_receiver = stack_fee_receiver
    }
    if let Some(stack_fee_commission) = param.stack_fee_commission {
        stack.stack_fee_commission = stack_fee_commission;
    }
    if let Some(total_stack_fee) = param.total_stack_fee {
        stack.total_stack_fee = total_stack_fee;
    }
    if let Some(new_admin) = param.new_admin {
        stack.admin = new_admin;
    }
    if let Some(lsd_token_code_id) = param.lsd_token_code_id {
        stack.lsd_token_code_id = lsd_token_code_id;
    }
    if let Some(add_entrusted_pool) = param.add_entrusted_pool {
        if !stack.pools.contains(&add_entrusted_pool) {
            stack.pools.push(add_entrusted_pool);
        }
    }

    STACK.save(deps.storage, &stack)?;

    Ok(Response::default())
}

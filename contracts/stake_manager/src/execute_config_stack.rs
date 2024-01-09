use cosmwasm_std::{DepsMut, MessageInfo, Response, StdError};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronError, NeutronResult,
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
    if let Some(add_operator) = param.add_operator {
        if stack.operators.contains(&add_operator) {
            return Err(NeutronError::Std(StdError::generic_err(
                "operator already exist",
            )));
        }
        stack.operators.push(add_operator);
    }
    if let Some(rm_operator) = param.rm_operator {
        if !stack.operators.contains(&rm_operator) {
            return Err(NeutronError::Std(StdError::generic_err(
                "operator not exist",
            )));
        }
        stack.operators.retain(|o| o != rm_operator);
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

    STACK.save(deps.storage, &stack)?;

    Ok(Response::default())
}

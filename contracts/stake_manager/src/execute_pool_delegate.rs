use crate::helper::{check_ibc_fee, DEFAULT_TIMEOUT_SECONDS};
use crate::state::EraStatus::ActiveEnded;
use crate::state::{INFO_OF_ICA_ID, POOLS};
use crate::{error_conversion::ContractError, helper::gen_delegation_txs};
use cosmwasm_std::{DepsMut, MessageInfo, Response, SubMsg, Uint128};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};
use std::ops::{Div, Mul, Sub};
use std::vec;

pub fn execute_pool_delegate(
    deps: DepsMut<NeutronQuery>,
    info: MessageInfo,
    pool_addr: String,
    stake_amount: Uint128,
) -> NeutronResult<Response<NeutronMsg>> {
    let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    // check era state
    if pool_info.status != ActiveEnded {
        return Err(ContractError::StatusNotAllow {}.into());
    }
    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }

    let validator_count = pool_info.validator_addrs.len() as u128;
    if validator_count == 0 {
        return Err(ContractError::ValidatorsEmpty {}.into());
    }

    let mut msgs = vec![];
    let amount_per_validator = stake_amount.div(Uint128::from(validator_count));
    let remainder = stake_amount.sub(amount_per_validator.mul(Uint128::new(validator_count)));

    for (index, validator_addr) in pool_info.validator_addrs.iter().enumerate() {
        let mut amount_for_this_validator = amount_per_validator;

        // Add the remainder to the first validator
        if index == 0 {
            amount_for_this_validator += remainder;
        }

        let any_msg = gen_delegation_txs(
            pool_addr.clone(),
            validator_addr.clone(),
            pool_info.remote_denom.clone(),
            amount_for_this_validator,
        );

        msgs.push(any_msg);
    }

    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

    let ibc_fee = check_ibc_fee(deps.as_ref(), &info)?;
    let cosmos_msg = NeutronMsg::submit_tx(
        pool_ica_info.ctrl_connection_id,
        pool_info.ica_id.clone(),
        msgs,
        "".to_string(),
        DEFAULT_TIMEOUT_SECONDS,
        ibc_fee,
    );

    let submsg = SubMsg::new(cosmos_msg);

    Ok(Response::default().add_submessage(submsg))
}

use cosmwasm_std::{DepsMut, Env, MessageInfo, Response};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

use crate::error_conversion::ContractError;
use crate::helper::deal_validators_icq_update;
use crate::state::{EraStatus, INFO_OF_ICA_ID, POOLS};

pub fn execute_add_pool_validators(
    deps: DepsMut<NeutronQuery>,
    _: Env,
    info: MessageInfo,
    pool_addr: String,
    validator_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }
    if pool_info.status != EraStatus::ActiveEnded {
        return Err(ContractError::EraProcessNotEnd {}.into());
    }

    if pool_info.validator_addrs.len() >= 5 {
        return Err(ContractError::ValidatorAddressesListSize {}.into());
    }
    if pool_info.validator_addrs.contains(&validator_addr) {
        return Err(ContractError::ValidatorAlreadyExit {}.into());
    }
    pool_info.validator_addrs.push(validator_addr);

    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;

    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

    deal_validators_icq_update(deps, pool_addr, pool_info, pool_ica_info.ctrl_connection_id)
}

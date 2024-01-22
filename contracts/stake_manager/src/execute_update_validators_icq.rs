use cosmwasm_std::{DepsMut, Env, MessageInfo, Response};

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

use crate::error_conversion::ContractError;
use crate::helper::deal_validators_icq_update;
use crate::state::INFO_OF_ICA_ID;
use crate::state::{ValidatorUpdateStatus, POOLS};

pub fn execute_update_validators_icq(
    deps: DepsMut<NeutronQuery>,
    _env: Env,
    info: MessageInfo,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info: crate::state::PoolInfo =
        POOLS.load(deps.as_ref().storage, pool_addr.clone())?;

    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }

    if pool_info.validator_update_status != ValidatorUpdateStatus::WaitQueryUpdate {
        return Err(ContractError::StatusNotAllow {}.into());
    }

    pool_info.validator_update_status = ValidatorUpdateStatus::End;
    POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;

    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

    deal_validators_icq_update(deps, pool_addr, pool_info, pool_ica_info.ctrl_connection_id)
}

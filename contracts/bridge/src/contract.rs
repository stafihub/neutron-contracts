use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::{
    get_proposal_id, BridgeInfo, Proposal, BRIDGE_INFO, PROPOSALS, RESOURCE_ID_TO_TOKEN,
};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
    Uint128, WasmMsg,
};
use cw2::set_contract_version;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:bridge";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    if (msg.relayers.len() as u64) < msg.threshold {
        return Err(ContractError::RelayersLenNotMatch {});
    }

    BRIDGE_INFO.save(
        deps.storage,
        &BridgeInfo {
            admin: msg.admin,
            threshold: msg.threshold,
            relayers: msg.relayers,
        },
    )?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::VoteProposal {
            chain_id,
            deposit_nonce,
            resource_id,
            recipient,
            amount,
        } => execute_vote_proposal(
            deps,
            info,
            chain_id,
            deposit_nonce,
            resource_id,
            recipient,
            amount,
        ),
        ExecuteMsg::AddRelayer { relayer } => execute_add_relayer(deps, info, relayer),
        ExecuteMsg::RemoveRelayer { relayer } => execute_remove_relayer(deps, info, relayer),
        ExecuteMsg::AddResourceIdToToken { resource_id, token } => {
            execute_add_resource_id_to_token(deps, info, resource_id, token)
        }
        ExecuteMsg::RemoveResourceId { resource_id } => {
            execute_remove_resource_id(deps, info, resource_id)
        }
        ExecuteMsg::ChangeThreshold { threshold } => {
            execute_change_threshold(deps, info, threshold)
        }
        ExecuteMsg::TransferAdmin { new_admin } => execute_transfer_admin(deps, info, new_admin),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::BridgeInfo {} => to_json_binary(&query_bridge_info(deps)?),
        QueryMsg::Proposal {
            chain_id,      // source chain id
            deposit_nonce, // deposit nonce from source chain
            resource_id,
            recipient,
            amount,
        } => to_json_binary(&query_proposal(
            deps,
            chain_id,
            deposit_nonce,
            resource_id,
            recipient,
            amount,
        )?),
    }
}

pub fn query_bridge_info(deps: Deps) -> StdResult<BridgeInfo> {
    let bridge_info = BRIDGE_INFO.load(deps.storage)?;

    Ok(bridge_info)
}

pub fn query_proposal(
    deps: Deps,
    chain_id: u64,      // source chain id
    deposit_nonce: u64, // deposit nonce from source chain
    resource_id: String,
    recipient: Addr,
    amount: Uint128,
) -> StdResult<Proposal> {
    let proposal = PROPOSALS.load(
        deps.storage,
        get_proposal_id(chain_id, deposit_nonce, &resource_id, recipient, amount),
    )?;

    Ok(proposal)
}

pub fn execute_vote_proposal(
    deps: DepsMut,
    info: MessageInfo,
    chain_id: u64,      // source chain id
    deposit_nonce: u64, // deposit nonce from source chain
    resource_id: String,
    recipient: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let bridge_info = BRIDGE_INFO.load(deps.storage)?;

    if !bridge_info.relayers.contains(&info.sender) {
        return Err(ContractError::Unauthorized {});
    }
    let proposal_id = get_proposal_id(
        chain_id,
        deposit_nonce,
        &resource_id,
        recipient.clone(),
        amount,
    );

    let mut proposal = if PROPOSALS.has(deps.storage, proposal_id.clone()) {
        let proposal = PROPOSALS.load(deps.storage, proposal_id.clone())?;
        proposal
    } else {
        Proposal {
            chain_id,
            deposit_nonce,
            resource_id,
            recipient,
            amount,
            executed: false,
            voters: vec![],
        }
    };

    if proposal.voters.contains(&info.sender) {
        return Err(ContractError::Duplicate {});
    }
    if proposal.executed {
        return Err(ContractError::AlreadyExecuted {});
    }

    proposal.voters.push(info.sender);

    let mut res = Response::new();
    if proposal.voters.len() as u64 >= bridge_info.threshold {
        let token_address =
            RESOURCE_ID_TO_TOKEN.load(deps.storage, proposal.resource_id.clone())?;

        let msg = WasmMsg::Execute {
            contract_addr: token_address.to_string(),
            msg: to_json_binary(
                &(lsd_token::msg::ExecuteMsg::Mint {
                    recipient: proposal.recipient.clone().into_string(),
                    amount: proposal.amount,
                }),
            )?,
            funds: vec![],
        };
        res = res.add_message(CosmosMsg::Wasm(msg));

        proposal.executed = true
    }

    PROPOSALS.save(deps.storage, proposal_id, &proposal)?;

    Ok(res
        .add_attribute("action", "vote_proposal")
        .add_attribute("executed", proposal.executed.to_string())
        .add_attribute("recipient", proposal.recipient)
        .add_attribute("chain_id", chain_id.to_string())
        .add_attribute("deposit_nonce", proposal.deposit_nonce.to_string())
        .add_attribute("resource_id", &proposal.resource_id)
        .add_attribute("amount", proposal.amount))
}

pub fn execute_add_relayer(
    deps: DepsMut,
    info: MessageInfo,
    relayer: Addr,
) -> Result<Response, ContractError> {
    let mut bridge_info = BRIDGE_INFO.load(deps.storage)?;
    if bridge_info.admin != info.sender {
        return Err(ContractError::Unauthorized {});
    }
    if bridge_info.relayers.contains(&relayer.clone()) {
        return Err(ContractError::Duplicate {});
    }
    bridge_info.relayers.push(relayer);

    BRIDGE_INFO.save(deps.storage, &bridge_info)?;

    Ok(Response::default().add_attribute("action", "add_relayer"))
}
pub fn execute_remove_relayer(
    deps: DepsMut,
    info: MessageInfo,
    relayer: Addr,
) -> Result<Response, ContractError> {
    let mut bridge_info = BRIDGE_INFO.load(deps.storage)?;
    if bridge_info.admin != info.sender {
        return Err(ContractError::Unauthorized {});
    }
    if !bridge_info.relayers.contains(&relayer.clone()) {
        return Err(ContractError::NotExist {});
    }
    bridge_info.relayers.retain(|r| r != relayer);

    if (bridge_info.relayers.len() as u64) < bridge_info.threshold {
        return Err(ContractError::RelayersLenNotMatch {});
    }

    BRIDGE_INFO.save(deps.storage, &bridge_info)?;

    Ok(Response::default().add_attribute("action", "remove_relayer"))
}

pub fn execute_add_resource_id_to_token(
    deps: DepsMut,
    info: MessageInfo,
    resource_id: String,
    token: Addr,
) -> Result<Response, ContractError> {
    let bridge_info = BRIDGE_INFO.load(deps.storage)?;
    if bridge_info.admin != info.sender {
        return Err(ContractError::Unauthorized {});
    }
    if RESOURCE_ID_TO_TOKEN.has(deps.storage, resource_id.clone()) {
        return Err(ContractError::Duplicate {});
    }

    RESOURCE_ID_TO_TOKEN.save(deps.storage, resource_id, &token)?;

    Ok(Response::default().add_attribute("action", "add_resource_id_to_token"))
}

pub fn execute_remove_resource_id(
    deps: DepsMut,
    info: MessageInfo,
    resource_id: String,
) -> Result<Response, ContractError> {
    let bridge_info = BRIDGE_INFO.load(deps.storage)?;
    if bridge_info.admin != info.sender {
        return Err(ContractError::Unauthorized {});
    }
    if !RESOURCE_ID_TO_TOKEN.has(deps.storage, resource_id.clone()) {
        return Err(ContractError::NotExist {});
    }

    RESOURCE_ID_TO_TOKEN.remove(deps.storage, resource_id);

    Ok(Response::default().add_attribute("action", "remove_resource_id"))
}

pub fn execute_change_threshold(
    deps: DepsMut,
    info: MessageInfo,
    threshold: u64,
) -> Result<Response, ContractError> {
    let mut bridge_info = BRIDGE_INFO.load(deps.storage)?;
    if bridge_info.admin != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    if (bridge_info.relayers.len() as u64) < threshold {
        return Err(ContractError::RelayersLenNotMatch {});
    }
    bridge_info.threshold = threshold;

    BRIDGE_INFO.save(deps.storage, &bridge_info)?;

    Ok(Response::default().add_attribute("action", "change_threshold"))
}

pub fn execute_transfer_admin(
    deps: DepsMut,
    info: MessageInfo,
    new_admin: String,
) -> Result<Response, ContractError> {
    let mut bridge_info = BRIDGE_INFO.load(deps.storage)?;
    if bridge_info.admin != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    if let Ok(admin) = deps.api.addr_validate(&new_admin) {
        bridge_info.admin = admin;
    } else {
        return Err(ContractError::InvalidAddress {});
    }

    BRIDGE_INFO.save(deps.storage, &bridge_info)?;

    Ok(Response::default().add_attribute("action", "transfer_admin"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

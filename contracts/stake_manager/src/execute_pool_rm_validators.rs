use std::{collections::HashSet, vec};

use cosmos_sdk_proto::cosmos::base::v1beta1::Coin;
use cosmos_sdk_proto::cosmos::staking::v1beta1::MsgBeginRedelegate;
use cosmos_sdk_proto::prost::Message;
use cosmwasm_std::{
    Binary, DepsMut, Env, MessageInfo, Response, StdError, StdResult, SubMsg, Uint128,
};

use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::interchain_queries::v045::new_register_delegator_delegations_query_msg;
use neutron_sdk::interchain_txs::helpers::get_port_id;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    interchain_queries::{
        check_query_type, get_registered_query, query_kv_result, types::QueryType,
        v045::types::Delegations,
    },
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronError, NeutronResult,
};

use crate::contract::DEFAULT_UPDATE_PERIOD;
use crate::state::{
    get_next_icq_reply_id, QueryKind, ValidatorUpdateStatus, ADDR_ICAID_MAP, POOLS,
};
use crate::{
    contract::{msg_with_sudo_callback, SudoPayload, TxType, DEFAULT_TIMEOUT_SECONDS},
    state::ADDR_DELEGATIONS_REPLY_ID,
};
use crate::{error_conversion::ContractError, state::REPLY_ID_TO_QUERY_ID};
use crate::{helper::min_ntrn_ibc_fee, state::POOL_VALIDATOR_STATUS};

pub fn execute_rm_pool_validators(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    pool_addr: String,
    validator_addrs: Vec<String>,
) -> NeutronResult<Response<NeutronMsg>> {
    let pool_info = POOLS.load(deps.as_ref().storage, pool_addr.clone())?;

    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_rm_pool_validators pool_info: {:?}",
            pool_info
        )
        .as_str(),
    );

    let mut pool_validator_status = POOL_VALIDATOR_STATUS.load(deps.storage, pool_addr.clone())?;
    if pool_validator_status.status == ValidatorUpdateStatus::Pending {
        deps.as_ref().api.debug(
            format!(
                "WASMDEBUG: execute_rm_pool_validators skip pool: {:?}",
                pool_addr
            )
            .as_str(),
        );
        return Err(NeutronError::Std(StdError::generic_err("status not allow")));
    }

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_rm_pool_validators pool_validator_status: {:?}",
            pool_validator_status
        )
        .as_str(),
    );

    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);

    // redelegate
    let contract_query_id = ADDR_DELEGATIONS_REPLY_ID.load(deps.storage, pool_addr.clone())?;
    let registered_query_id = REPLY_ID_TO_QUERY_ID.load(deps.storage, contract_query_id)?;

    let remove_msg_old_query = NeutronMsg::remove_interchain_query(registered_query_id);

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_rm_pool_validators contract_query_id: {:?} registered_query_id:{:?}",
            contract_query_id, registered_query_id
        )
            .as_str(),
    );

    let interchain_account_id = ADDR_ICAID_MAP.load(deps.storage, pool_addr.clone())?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_rm_pool_validators interchain_account_id: {:?}",
            interchain_account_id
        )
        .as_str(),
    );

    // get info about the query
    let registered_query = get_registered_query(deps.as_ref(), registered_query_id)?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_rm_pool_validators registered_query:{:?}",
            registered_query
        )
        .as_str(),
    );

    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Delegations structure from raw KV-storage values
    let delegations: Delegations = query_kv_result(deps.as_ref(), registered_query_id)?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_rm_pool_validators delegations: {:?}",
            delegations
        )
        .as_str(),
    );

    let target_validator = match find_redelegation_target(&delegations, &validator_addrs) {
        Some(target_validator) => target_validator,
        None => {
            return Err(NeutronError::Std(StdError::generic_err(
                "find_redelegation_target failed",
            )));
        }
    };

    let mut msgs = vec![];

    for src_validator in validator_addrs.clone() {
        let amount = match find_validator_amount(&delegations, src_validator.clone()) {
            Some(amount) => amount,
            None => {
                continue;
            }
        };
        // add sub message to redelegate
        let redelegate_msg = MsgBeginRedelegate {
            delegator_address: pool_addr.clone(),
            validator_src_address: src_validator.clone(),
            validator_dst_address: target_validator.clone(),
            amount: Some(Coin {
                denom: pool_info.ibc_denom.clone(),
                amount: amount.to_string(),
            }),
        };
        let mut buf = Vec::new();
        buf.reserve(redelegate_msg.encoded_len());

        if let Err(e) = redelegate_msg.encode(&mut buf) {
            return Err(NeutronError::Std(StdError::generic_err(format!(
                "Encode error: {}",
                e
            ))));
        }

        let any_msg = ProtobufAny {
            type_url: "/cosmos.staking.v1beta1.BeginRedelegate".to_string(),
            value: Binary::from(buf),
        };

        msgs.push(any_msg);
    }

    let cosmos_msg = NeutronMsg::submit_tx(
        pool_info.connection_id.clone(),
        interchain_account_id.clone(),
        msgs,
        "".to_string(),
        DEFAULT_TIMEOUT_SECONDS,
        fee.clone(),
    );

    let rm_validators_set: HashSet<_> = validator_addrs.clone().into_iter().collect();
    let now_validators_set: HashSet<_> = pool_info.validator_addrs.into_iter().collect();

    // Find the difference
    let difference: HashSet<_> = now_validators_set.difference(&rm_validators_set).collect();
    let vec_of_strings: Vec<String> = difference.into_iter().cloned().collect();

    let new_validator_list_str = vec_of_strings
        .clone()
        .iter()
        .map(|index| index.to_string())
        .collect::<Vec<String>>()
        .join("_");

    // We use a submessage here because we need the process message reply to save
    // the outgoing IBC packet identifier for later.
    let submsg_redelegate = msg_with_sudo_callback(
        deps.branch(),
        cosmos_msg,
        SudoPayload {
            port_id: get_port_id(
                env.contract.address.to_string(),
                interchain_account_id.clone(),
            ),
            pool_addr: pool_info.pool_addr.clone(),
            message: new_validator_list_str,
            tx_type: TxType::RmValidator,
        },
    )?;

    let register_delegation_query_msg = new_register_delegator_delegations_query_msg(
        pool_info.connection_id.clone(),
        pool_addr.clone(),
        vec_of_strings,
        DEFAULT_UPDATE_PERIOD,
    )?;

    let next_icq_reply_id = get_next_icq_reply_id(deps.storage, QueryKind::Delegations)?;
    let register_delegation_query_submsg =
        SubMsg::reply_on_success(register_delegation_query_msg, next_icq_reply_id);

    pool_validator_status.status = ValidatorUpdateStatus::Pending;
    POOL_VALIDATOR_STATUS.save(deps.storage, pool_addr, &pool_validator_status)?;

    Ok(Response::default()
        .add_message(remove_msg_old_query)
        .add_submessage(register_delegation_query_submsg)
        .add_submessage(submsg_redelegate))
}

// todo: What if submsg_redelegate fails when the old delegation query has been removed

fn find_validator_amount(delegations: &Delegations, validator_address: String) -> Option<Uint128> {
    for delegation in &delegations.delegations {
        if delegation.validator == validator_address {
            return Some(delegation.amount.amount);
        }
    }
    None
}

fn find_redelegation_target(
    delegations: &Delegations,
    excluded_validators: &[String],
) -> Option<String> {
    // Find the validator from delegations that is not in excluded_validators and has the smallest delegate count
    let mut min_delegation: Option<(String, Uint128)> = None;

    for delegation in &delegations.delegations {
        // Skip the validators in excluded_validators
        if excluded_validators.contains(&delegation.validator) {
            continue;
        }

        // Update the minimum delegation validator
        match min_delegation {
            Some((_, min_amount)) if delegation.amount.amount < min_amount => {
                min_delegation = Some((delegation.validator.clone(), delegation.amount.amount));
            }
            None => {
                min_delegation = Some((delegation.validator.clone(), delegation.amount.amount));
            }
            _ => {}
        }
    }

    min_delegation.map(|(validator, _)| validator)
}

pub fn sudo_rm_validator_callback(deps: DepsMut, payload: SudoPayload) -> StdResult<Response> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;
    let mut pool_validator_status =
        POOL_VALIDATOR_STATUS.load(deps.storage, payload.pool_addr.clone())?;

    let new_validators: Vec<String> = payload.message.split('_').map(String::from).collect();

    pool_info.validator_addrs = new_validators;
    pool_validator_status.status = ValidatorUpdateStatus::Success;

    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;
    POOL_VALIDATOR_STATUS.save(
        deps.storage,
        payload.pool_addr.clone(),
        &pool_validator_status,
    )?;

    Ok(Response::new())
}

pub fn sudo_rm_validator_failed_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> StdResult<Response> {
    let mut pool_validator_status =
        POOL_VALIDATOR_STATUS.load(deps.storage, payload.pool_addr.clone())?;

    pool_validator_status.status = ValidatorUpdateStatus::Failed;

    POOL_VALIDATOR_STATUS.save(deps.storage, payload.pool_addr, &pool_validator_status)?;
    Ok(Response::new())
}

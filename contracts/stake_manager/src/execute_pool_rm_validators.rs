use std::vec;

use cosmos_sdk_proto::cosmos::base::v1beta1::Coin;
use cosmos_sdk_proto::cosmos::staking::v1beta1::MsgBeginRedelegate;
use cosmos_sdk_proto::prost::Message;
use cosmwasm_std::{ Binary, DepsMut, Env, MessageInfo, Response, StdError, Uint128 };
use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::interchain_txs::helpers::get_port_id;
use neutron_sdk::{
    bindings::{ msg::NeutronMsg, query::NeutronQuery },
    interchain_queries::{
        check_query_type,
        get_registered_query,
        query_kv_result,
        types::QueryType,
        v045::types::Delegations,
    },
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronError,
    NeutronResult,
};

use crate::contract::{ msg_with_sudo_callback, SudoPayload, TxType, DEFAULT_TIMEOUT_SECONDS };
use crate::state::ADDR_QUERY_ID;
use crate::state::{ POOLS, POOL_ICA_MAP };

pub fn execute_rm_pool_validators(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    _: MessageInfo,
    pool_addr: String,
    validator_addrs: Vec<String>
) -> NeutronResult<Response<NeutronMsg>> {
    let fee = crate::contract::min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);

    // redelegate
    let registered_query_id = ADDR_QUERY_ID.load(deps.storage, pool_addr.clone())?;
    let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    let interchain_account_id = POOL_ICA_MAP.load(deps.storage, pool_addr.clone())?;
    // get info about the query
    let registered_query = get_registered_query(deps.as_ref(), registered_query_id)?;
    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Delegations structure from raw KV-storage values
    let delegations: Delegations = query_kv_result(deps.as_ref(), registered_query_id)?;

    let target_validator = match find_redelegation_target(&delegations, &validator_addrs) {
        Some(target_validator) => target_validator,
        None => {
            return Err(NeutronError::Std(StdError::generic_err("find_redelegation_target failed")));
        }
    };

    let mut msgs = vec![];

    for src_validator in validator_addrs {
        let amount = match find_validator_amount(&delegations, src_validator.clone()) {
            Some(amount) => amount,
            None => {
                continue;
            }
        };
        // add submessage to unstake
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
            return Err(NeutronError::Std(StdError::generic_err(format!("Encode error: {}", e))));
        }

        let any_msg = ProtobufAny {
            type_url: "/cosmos.staking.v1beta1.MsgUndelegate".to_string(),
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
        fee.clone()
    );

    // We use a submessage here because we need the process message reply to save
    // the outgoing IBC packet identifier for later.
    let submsg_redelegate = msg_with_sudo_callback(deps.branch(), cosmos_msg, SudoPayload {
        port_id: get_port_id(env.contract.address.to_string(), interchain_account_id.clone()),
        message: "interchain_undelegate".to_string(),
        tx_type: TxType::RmValidator,
    })?;

    // todo: update state in sudo reply
    // todo: update delegation_query in sudo reply
    // todo: update pool validator list
    Ok(Response::default().add_submessage(submsg_redelegate))
}

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
    excluded_validators: &[String]
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

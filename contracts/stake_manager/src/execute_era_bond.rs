use std::ops::{Div, Mul, Sub};
use std::vec;

use cosmos_sdk_proto::cosmos::base::v1beta1::Coin;
use cosmos_sdk_proto::cosmos::staking::v1beta1::{MsgDelegate, MsgUndelegate};
use cosmos_sdk_proto::prost::Message;
use cosmwasm_std::{Binary, DepsMut, Env, Response, StdError, Uint128};
use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::interchain_queries::v045::types::Delegations;
use neutron_sdk::interchain_txs::helpers::get_port_id;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronError, NeutronResult,
};

use crate::contract::{SudoPayload, TxType, DEFAULT_TIMEOUT_SECONDS};
use crate::query::query_delegation_by_addr;
use crate::state::PoolBondState::EraUpdated;
use crate::state::{POOLS, POOL_ICA_MAP};

#[derive(Clone, Debug)]
struct ValidatorUnbondInfo {
    pub validator: String,
    pub delegation_amount: Uint128,
    pub unbond_amount: Uint128,
}

pub fn execute_era_bond(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    // --------------------------------------------------------------------------------------------------
    // contract must pay for relaying of acknowledgements
    // See more info here: https://docs.neutron.org/neutron/feerefunder/overview
    let fee = crate::contract::min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let mut msgs = vec![];
    let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    // check era state
    if pool_info.era_update_status != EraUpdated {
        deps.as_ref()
            .api
            .debug(format!("WASMDEBUG: execute_era_bond skip pool: {:?}", pool_addr).as_str());
        return Ok(Response::new());
    }

    let interchain_account_id = POOL_ICA_MAP.load(deps.storage, pool_addr.clone())?;
    if pool_info.unbond > pool_info.active {
        let unbond_amount = pool_info.unbond - pool_info.active;

        let delegations = query_delegation_by_addr(deps.as_ref(), pool_addr.clone())?;

        let unbond_infos = allocate_unbond_amount(&delegations, unbond_amount);
        for info in unbond_infos {
            println!(
                "Validator: {}, Delegation: {}, Unbond: {}",
                info.validator, info.delegation_amount, info.unbond_amount
            );

            // add submessage to unstake
            let delegate_msg = MsgUndelegate {
                delegator_address: pool_addr.clone(),
                validator_address: info.validator.clone(),
                amount: Some(Coin {
                    denom: pool_info.ibc_denom.clone(),
                    amount: info.unbond_amount.to_string(),
                }),
            };
            let mut buf = Vec::new();
            buf.reserve(delegate_msg.encoded_len());

            if let Err(e) = delegate_msg.encode(&mut buf) {
                return Err(NeutronError::Std(StdError::generic_err(format!(
                    "Encode error: {}",
                    e
                ))));
            }

            let any_msg = ProtobufAny {
                type_url: "/cosmos.staking.v1beta1.MsgUndelegate".to_string(),
                value: Binary::from(buf),
            };

            let cosmos_msg = NeutronMsg::submit_tx(
                pool_info.connection_id.clone(),
                interchain_account_id.clone(),
                vec![any_msg],
                "".to_string(),
                DEFAULT_TIMEOUT_SECONDS,
                fee.clone(),
            );

            // We use a submessage here because we need the process message reply to save
            // the outgoing IBC packet identifier for later.
            let submsg_unstake = crate::contract::msg_with_sudo_callback(
                deps.branch(),
                cosmos_msg,
                SudoPayload {
                    port_id: get_port_id(
                        env.contract.address.to_string(),
                        interchain_account_id.clone(),
                    ),
                    // Here you can store some information about the transaction to help you parse
                    // the acknowledgement later.
                    message: "interchain_undelegate".to_string(),
                    tx_type: TxType::EraBondUnstake,
                },
            )?;

            msgs.push(submsg_unstake);
        }
    } else if pool_info.active > pool_info.need_withdraw {
        let stake_amount = pool_info.active - pool_info.need_withdraw;

        let validator_count = pool_info.validator_addrs.len() as u128;

        if validator_count == 0 {
            return Err(NeutronError::Std(StdError::generic_err(
                "validator_count is zero",
            )));
        }

        let amount_per_validator = stake_amount.div(Uint128::from(validator_count));
        let remainder = stake_amount.sub(amount_per_validator.mul(amount_per_validator));

        for (index, validator_addr) in pool_info.validator_addrs.iter().enumerate() {
            let mut amount_for_this_validator = amount_per_validator;

            // Add the remainder to the first validator
            if index == 0 {
                amount_for_this_validator += remainder;
            }

            // add submessage to stake
            let delegate_msg = MsgDelegate {
                delegator_address: pool_addr.clone(),
                validator_address: validator_addr.clone(),
                amount: Some(Coin {
                    denom: pool_info.ibc_denom.clone(),
                    amount: amount_for_this_validator.to_string(),
                }),
            };

            // Serialize the Delegate message.
            let mut buf = Vec::new();
            buf.reserve(delegate_msg.encoded_len());

            if let Err(e) = delegate_msg.encode(&mut buf) {
                return Err(NeutronError::Std(StdError::generic_err(format!(
                    "Encode error: {}",
                    e
                ))));
            }

            // Put the serialized Delegate message to a types.Any protobuf message.
            let any_msg = ProtobufAny {
                type_url: "/cosmos.staking.v1beta1.MsgDelegate".to_string(),
                value: Binary::from(buf),
            };

            // Form the neutron SubmitTx message containing the binary Delegate message.
            let cosmos_msg = NeutronMsg::submit_tx(
                pool_info.connection_id.clone(),
                interchain_account_id.clone(),
                vec![any_msg],
                "".to_string(),
                DEFAULT_TIMEOUT_SECONDS,
                fee.clone(),
            );

            // We use a submessage here because we need the process message reply to save
            // the outgoing IBC packet identifier for later.
            let submsg_stake = crate::contract::msg_with_sudo_callback(
                deps.branch(),
                cosmos_msg,
                SudoPayload {
                    port_id: get_port_id(
                        env.contract.address.to_string(),
                        interchain_account_id.clone(),
                    ),
                    // Here you can store some information about the transaction to help you parse
                    // the acknowledgement later.
                    message: "interchain_delegate".to_string(),
                    tx_type: TxType::EraBondStake,
                },
            )?;
            msgs.push(submsg_stake);
        }
    }

    Ok(Response::default().add_submessages(msgs))
}

fn allocate_unbond_amount(
    delegations: &Delegations,
    unbond_amount: Uint128,
) -> Vec<ValidatorUnbondInfo> {
    let mut unbond_infos: Vec<ValidatorUnbondInfo> = Vec::new();
    let mut remaining_unbond = unbond_amount;

    // Sort the delegations by amount in descending order
    let mut sorted_delegations = delegations.delegations.clone();
    sorted_delegations.sort_by(|a, b| b.amount.amount.cmp(&a.amount.amount));

    for delegation in sorted_delegations.iter() {
        if remaining_unbond.is_zero() {
            break;
        }

        let mut current_unbond = remaining_unbond;

        // If the current validator delegate amount is less than the remaining delegate amount, all are discharged
        if delegation.amount.amount < remaining_unbond {
            current_unbond = delegation.amount.amount;
        }

        remaining_unbond -= current_unbond;

        unbond_infos.push(ValidatorUnbondInfo {
            validator: delegation.validator.clone(),
            delegation_amount: delegation.amount.amount,
            unbond_amount: current_unbond,
        });
    }

    unbond_infos
}

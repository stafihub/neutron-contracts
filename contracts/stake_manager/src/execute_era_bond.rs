use std::vec;
use std::{
    collections::HashSet,
    ops::{Div, Mul, Sub},
};

use cosmos_sdk_proto::cosmos::staking::v1beta1::MsgUndelegate;
use cosmos_sdk_proto::cosmos::{
    base::v1beta1::Coin, distribution::v1beta1::MsgWithdrawDelegatorReward,
};
use cosmos_sdk_proto::prost::Message;
use cosmwasm_std::{Binary, Delegation, DepsMut, Env, Response, Uint128};

use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronResult,
};

use crate::helper::STAKE_SPLIT_THRESHOLD;
use crate::state::EraStatus::{BondEnded, BondStarted, EraUpdateEnded};
use crate::state::{SudoPayload, TxType, INFO_OF_ICA_ID, POOLS, VALIDATORS_UNBONDS_TIME};
use crate::tx_callback::msg_with_sudo_callback;
use crate::{
    error_conversion::ContractError,
    helper::{gen_delegation_txs, min_ntrn_ibc_fee},
};
use crate::{helper::DEFAULT_TIMEOUT_SECONDS, query::query_delegation_by_addr};

#[derive(Clone, Debug)]
struct ValidatorUnbondInfo {
    pub validator: String,
    pub unbond_amount: Uint128,
}

pub fn execute_era_bond(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    // check era state
    if pool_info.status != EraUpdateEnded {
        return Err(ContractError::StatusNotAllow {}.into());
    }

    let mut msgs = vec![];

    let mut msg_str = "".to_string();
    if pool_info.era_snapshot.unbond >= pool_info.era_snapshot.bond {
        let unbond_amount = pool_info
            .era_snapshot
            .unbond
            .sub(pool_info.era_snapshot.bond);

        let delegations = query_delegation_by_addr(
            deps.as_ref(),
            pool_addr.clone(),
            pool_info.sdk_greater_or_equal_v047,
        )?;
        if delegations.last_submitted_local_height <= pool_info.era_snapshot.last_step_height {
            return Err(ContractError::DelegationSubmissionHeight {}.into());
        }
        let delegating_vals: Vec<String> = delegations
            .delegations
            .iter()
            .map(|delegation| delegation.validator.clone())
            .collect();

        let mut op_validators = vec![];
        if unbond_amount.u128() > 0 {
            let unbond_infos = allocate_unbond_amount(
                deps.branch(),
                env.block.time.seconds(),
                &delegations.delegations,
                unbond_amount,
                pool_info.unbonding_period * pool_info.era_seconds,
            )?;
            if unbond_infos.is_empty() {
                return Err(ContractError::ValidatorForUnbondNotEnough {}.into());
            }
            let unbond_validators: Vec<String> =
                unbond_infos.iter().map(|u| u.validator.clone()).collect();
            msg_str = unbond_validators.join("_");

            for info in unbond_infos {
                op_validators.push(info.validator.clone());

                // add submessage to unstake
                let delegate_msg = MsgUndelegate {
                    delegator_address: pool_addr.clone(),
                    validator_address: info.validator.clone(),
                    amount: Some(Coin {
                        denom: pool_info.remote_denom.clone(),
                        amount: info.unbond_amount.to_string(),
                    }),
                };
                let mut buf = Vec::new();
                buf.reserve(delegate_msg.encoded_len());

                if let Err(e) = delegate_msg.encode(&mut buf) {
                    return Err(ContractError::EncodeError(e.to_string()).into());
                }

                let any_msg = ProtobufAny {
                    type_url: "/cosmos.staking.v1beta1.MsgUndelegate".to_string(),
                    value: Binary::from(buf),
                };

                msgs.push(any_msg);
            }
        }
        // Check whether the delegator-validator needs to manually withdraw
        if op_validators.len() != delegating_vals.len() {
            // Find the difference between delegation validator_addrs and op_validators
            let pool_validators: HashSet<_> = delegating_vals.into_iter().collect();
            let op_validators_set: HashSet<_> = op_validators.into_iter().collect();

            // Find the difference
            let difference: HashSet<_> = pool_validators.difference(&op_validators_set).collect();

            // Convert the difference back to Vec
            let difference_vec: Vec<_> = difference.into_iter().collect();
            for validator_addr in difference_vec {
                // Create a MsgWithdrawDelegatorReward message
                let withdraw_msg = MsgWithdrawDelegatorReward {
                    delegator_address: pool_addr.clone(),
                    validator_address: validator_addr.clone(),
                };

                // Serialize the MsgWithdrawDelegatorReward message
                let mut buf = Vec::new();
                buf.reserve(withdraw_msg.encoded_len());

                if let Err(e) = withdraw_msg.encode(&mut buf) {
                    return Err(ContractError::EncodeError(e.to_string()).into());
                }

                // Put the serialized MsgWithdrawDelegatorReward message to a types.Any protobuf message
                let any_msg = ProtobufAny {
                    type_url: "/cosmos.distribution.v1beta1.MsgWithdrawDelegatorReward".to_string(),
                    value: Binary::from(buf),
                };

                // Form the neutron SubmitTx message containing the binary MsgWithdrawDelegatorReward message
                msgs.push(any_msg);
            }
        }
    } else {
        let stake_amount = pool_info.era_snapshot.bond - pool_info.era_snapshot.unbond;
        let validator_count = pool_info.validator_addrs.len() as u128;
        if validator_count == 0 {
            return Err(ContractError::ValidatorsEmpty {}.into());
        }
        if stake_amount < STAKE_SPLIT_THRESHOLD {
            for (index, validator_addr) in pool_info.validator_addrs.iter().enumerate() {
                if index == 0 {
                    msgs.push(gen_delegation_txs(
                        pool_addr.clone(),
                        validator_addr.clone(),
                        pool_info.remote_denom.clone(),
                        stake_amount,
                    ));
                } else {
                    let withdraw_msg = MsgWithdrawDelegatorReward {
                        delegator_address: pool_addr.clone(),
                        validator_address: validator_addr.clone(),
                    };

                    let mut buf = Vec::new();
                    buf.reserve(withdraw_msg.encoded_len());

                    if let Err(e) = withdraw_msg.encode(&mut buf) {
                        return Err(ContractError::EncodeError(e.to_string()).into());
                    }

                    msgs.push(ProtobufAny {
                        type_url: "/cosmos.distribution.v1beta1.MsgWithdrawDelegatorReward"
                            .to_string(),
                        value: Binary::from(buf),
                    });
                }
            }
        } else {
            let amount_per_validator = stake_amount.div(Uint128::from(validator_count));
            let remainder =
                stake_amount.sub(amount_per_validator.mul(Uint128::new(validator_count)));

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
        }
    }

    let (pool_ica_info, _, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let cosmos_msg = NeutronMsg::submit_tx(
        pool_ica_info.ctrl_connection_id,
        pool_info.ica_id.clone(),
        msgs,
        "".to_string(),
        DEFAULT_TIMEOUT_SECONDS,
        fee,
    );

    let submsg = msg_with_sudo_callback(
        deps.branch(),
        cosmos_msg,
        SudoPayload {
            port_id: pool_ica_info.ctrl_port_id,
            // the acknowledgement later
            message: msg_str,
            pool_addr: pool_addr.clone(),
            tx_type: TxType::EraBond,
        },
    )?;

    pool_info.status = BondStarted;
    POOLS.save(deps.storage, pool_addr, &pool_info)?;

    Ok(Response::default().add_submessage(submsg))
}

fn allocate_unbond_amount(
    deps: DepsMut<NeutronQuery>,
    current_time: u64,
    delegations: &[Delegation],
    unbond_amount: Uint128,
    unbonding_period_seconds: u64,
) -> NeutronResult<Vec<ValidatorUnbondInfo>> {
    let mut unbond_infos: Vec<ValidatorUnbondInfo> = Vec::new();
    let mut remaining_unbond = unbond_amount;

    // Sort the delegations by amount in descending order
    let mut sorted_delegations = delegations.to_vec();
    sorted_delegations.sort_by(|a, b| b.amount.amount.cmp(&a.amount.amount));
    for delegation in sorted_delegations.iter() {
        if remaining_unbond.is_zero() {
            break;
        }

        // clear timestamps
        if let Some(mut timestamps) = VALIDATORS_UNBONDS_TIME.may_load(
            deps.storage,
            (
                delegation.delegator.to_string(),
                delegation.validator.clone(),
            ),
        )? {
            if !timestamps.is_empty() {
                let oldest_record_time = timestamps[0];
                if current_time > oldest_record_time + unbonding_period_seconds {
                    timestamps.remove(0);

                    VALIDATORS_UNBONDS_TIME.save(
                        deps.storage,
                        (
                            delegation.delegator.to_string(),
                            delegation.validator.clone(),
                        ),
                        &timestamps,
                    )?;
                }

                if timestamps.len() >= 7 {
                    continue;
                }
            }
        }

        let mut current_unbond = remaining_unbond;
        // If the current validator delegate amount is less than the remaining delegate amount, all are discharged
        if delegation.amount.amount < remaining_unbond {
            current_unbond = delegation.amount.amount;
        }

        remaining_unbond -= current_unbond;
        unbond_infos.push(ValidatorUnbondInfo {
            validator: delegation.validator.clone(),
            unbond_amount: current_unbond,
        });
    }

    if !remaining_unbond.is_zero() {
        return Err(ContractError::ValidatorForUnbondNotEnough {}.into());
    }

    Ok(unbond_infos)
}

pub fn sudo_era_bond_callback(
    deps: DepsMut,
    env: Env,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;
    if payload.message.len() > 0 {
        let unbond_validators: Vec<String> = payload.message.split("_").map(String::from).collect();
        let timestamp = env.block.time.seconds();
        for unbond_validator in unbond_validators {
            let timestamps_op = VALIDATORS_UNBONDS_TIME.may_load(
                deps.storage,
                (payload.pool_addr.clone(), unbond_validator.clone()),
            )?;
            let final_timestamps = if let Some(mut timestamps) = timestamps_op {
                timestamps.push(timestamp);
                timestamps
            } else {
                vec![timestamp]
            };
            VALIDATORS_UNBONDS_TIME.save(
                deps.storage,
                (payload.pool_addr.clone(), unbond_validator),
                &final_timestamps,
            )?;
        }
    }

    pool_info.status = BondEnded;
    pool_info.era_snapshot.last_step_height = env.block.height;
    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;

    Ok(Response::new())
}

pub fn sudo_era_bond_failed_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;
    pool_info.status = EraUpdateEnded;
    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;

    Ok(Response::new())
}

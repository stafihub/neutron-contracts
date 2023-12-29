use core::ops::Sub;

use cosmos_sdk_proto::cosmos::bank::v1beta1::MsgSend;
use cosmos_sdk_proto::prost::Message;
use cosmwasm_std::{Binary, DepsMut, Env, Response, StdError, StdResult, Uint128};
use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    interchain_txs::helpers::get_port_id,
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronError, NeutronResult,
};

use crate::contract::DEFAULT_TIMEOUT_SECONDS;
use crate::helper::min_ntrn_ibc_fee;
use crate::state::PoolBondState::{BondReported, WithdrawReported};
use crate::state::POOLS;
use crate::{
    contract::{msg_with_sudo_callback, SudoPayload, TxType},
    query::query_balance_by_addr,
    state::ADDR_ICAID_MAP,
};

pub fn execute_era_collect_withdraw(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;
    // check era state
    if pool_info.era_update_status != BondReported {
        deps.as_ref().api.debug(
            format!(
                "WASMDEBUG: execute_era_collect_withdraw skip pool: {:?}",
                pool_addr
            )
            .as_str(),
        );
        return Err(NeutronError::Std(StdError::generic_err("status not allow")));
    }

    // check withdraw address balance and send it to the pool
    let withdraw_balances_result =
        query_balance_by_addr(deps.as_ref(), pool_info.withdraw_addr.clone());

    let mut withdraw_amount = Uint128::zero();
    match withdraw_balances_result {
        Ok(balance_response) => {
            if !balance_response.balances.coins.is_empty() {
                withdraw_amount = balance_response
                    .balances
                    .coins
                    .iter()
                    .find(|c| c.denom == pool_info.remote_denom.clone())
                    .map(|c| c.amount)
                    .unwrap_or(Uint128::zero());
            }
        }
        Err(_) => {
            // return Err(NeutronError::Std(StdError::generic_err(
            //     "balance not exist",
            // )));
            deps.as_ref().api.debug(
                format!(
                    "WASMDEBUG: execute_era_collect_withdraw withdraw_balances_result: {:?}",
                    withdraw_balances_result
                )
                .as_str(),
            );
        }
    }

    // leave gas
    if withdraw_amount < Uint128::new(1000000) {
        pool_info.era_update_status = WithdrawReported;
        POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;
        return Ok(Response::default());
    }
    // Leave 0.4 atom for gas
    // magic number can change it to a configuration item later
    withdraw_amount = withdraw_amount.sub(Uint128::new(400000));

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_era_collect_withdraw withdraw_amount: {:?}",
            withdraw_amount
        )
        .as_str(),
    );

    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);

    let tx_withdraw_coin = cosmos_sdk_proto::cosmos::base::v1beta1::Coin {
        denom: pool_info.remote_denom.clone(),
        amount: withdraw_amount.to_string(),
    };

    let inter_send = MsgSend {
        from_address: pool_info.withdraw_addr.clone(),
        to_address: pool_addr.clone(),
        amount: vec![tx_withdraw_coin],
    };

    let mut buf = Vec::new();
    buf.reserve(inter_send.encoded_len());

    if let Err(e) = inter_send.encode(&mut buf) {
        return Err(NeutronError::Std(StdError::generic_err(format!(
            "Encode error: {}",
            e
        ))));
    }

    let any_msg = ProtobufAny {
        type_url: "/cosmos.bank.v1beta1.MsgSend".to_string(),
        value: Binary::from(buf),
    };

    let interchain_account_id = ADDR_ICAID_MAP.load(deps.storage, pool_addr.clone())?;
    let cosmos_msg = NeutronMsg::submit_tx(
        pool_info.connection_id.clone(),
        interchain_account_id.clone(),
        vec![any_msg],
        "".to_string(),
        DEFAULT_TIMEOUT_SECONDS,
        fee.clone(),
    );

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_era_collect_withdraw cosmos_msg: {:?}",
            cosmos_msg
        )
        .as_str(),
    );

    let submsg = msg_with_sudo_callback(
        deps.branch(),
        cosmos_msg,
        SudoPayload {
            port_id: get_port_id(
                env.contract.address.to_string(),
                interchain_account_id.clone(),
            ),
            // the acknowledgement later
            message: "".to_string(),
            pool_addr: pool_addr.clone(),
            tx_type: TxType::EraBond,
        },
    )?;

    Ok(Response::default().add_submessage(submsg))
}

pub fn sudo_era_collect_withdraw_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> StdResult<Response> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr)?;
    pool_info.era_update_status = WithdrawReported;
    POOLS.save(deps.storage, pool_info.pool_addr.clone(), &pool_info)?;
    Ok(Response::new())
}

use cosmos_sdk_proto::cosmos::bank::v1beta1::MsgSend;
use cosmos_sdk_proto::prost::Message;
use cosmwasm_std::{Binary, DepsMut, Env, Response, StdError, StdResult, Uint128};

use neutron_sdk::bindings::types::ProtobufAny;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    NeutronError, NeutronResult,
};

use crate::helper::{get_withdraw_ica_id, min_ntrn_ibc_fee};
use crate::state::EraProcessStatus::{BondEnded, WithdrawEnded, WithdrawStarted};
use crate::state::{INFO_OF_ICA_ID, POOLS};
use crate::{contract::DEFAULT_TIMEOUT_SECONDS, state::POOL_ERA_SHOT};
use crate::{
    contract::{msg_with_sudo_callback, SudoPayload, TxType},
    query::query_balance_by_addr,
};

pub fn execute_era_collect_withdraw(
    mut deps: DepsMut<NeutronQuery>,
    _: Env,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    // check era state
    if pool_info.era_process_status != BondEnded {
        deps.as_ref().api.debug(
            format!(
                "WASMDEBUG: execute_era_collect_withdraw skip pool: {:?}",
                pool_addr
            )
            .as_str(),
        );
        return Err(NeutronError::Std(StdError::generic_err("status not allow")));
    }
    pool_info.era_process_status = WithdrawStarted;

    let (_, withdraw_ica_info) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

    // check withdraw address balance and send it to the pool
    let withdraw_balances_result: Result<
        neutron_sdk::interchain_queries::v045::queries::BalanceResponse,
        NeutronError,
    > = query_balance_by_addr(deps.as_ref(), withdraw_ica_info.ica_addr.clone());

    let mut pool_era_shot = POOL_ERA_SHOT.load(deps.storage, pool_addr.clone())?;

    let mut withdraw_amount = Uint128::zero();
    match withdraw_balances_result {
        Ok(balance_response) => {
            if balance_response.last_submitted_local_height <= pool_era_shot.bond_height {
                return Err(NeutronError::Std(StdError::generic_err("Withdraw Addr submission height is less than or equal to the bond height of the pool era, which is not allowed.")));
            }

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
    if withdraw_amount.is_zero() {
        pool_info.era_process_status = WithdrawEnded;
        POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;

        return Ok(Response::default());
    }

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_era_collect_withdraw withdraw_amount: {:?}",
            withdraw_amount
        )
        .as_str(),
    );

    // let interchain_account_id =
    //     ADDR_ICAID_MAP.load(deps.storage, pool_info.withdraw_addr.clone())?;

    let tx_withdraw_coin = cosmos_sdk_proto::cosmos::base::v1beta1::Coin {
        denom: pool_info.remote_denom.clone(),
        amount: withdraw_amount.to_string(),
    };

    let inter_send = MsgSend {
        from_address: withdraw_ica_info.ica_addr.clone(),
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

    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let cosmos_msg = NeutronMsg::submit_tx(
        withdraw_ica_info.ctrl_connection_id.clone(),
        get_withdraw_ica_id(pool_info.ica_id),
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
            port_id: withdraw_ica_info.ctrl_port_id,
            message: "".to_string(),
            pool_addr: pool_addr.clone(),
            tx_type: TxType::EraCollectWithdraw,
        },
    )?;

    pool_era_shot.restake_amount = withdraw_amount;
    POOL_ERA_SHOT.save(deps.storage, pool_addr, &pool_era_shot)?;

    Ok(Response::default().add_submessage(submsg))
}

pub fn sudo_era_collect_withdraw_callback(
    deps: DepsMut,
    env: Env,
    payload: SudoPayload,
) -> StdResult<Response> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;
    pool_info.era_process_status = WithdrawEnded;
    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;

    let mut pool_era_shot = POOL_ERA_SHOT.load(deps.storage, payload.pool_addr.clone())?;
    pool_era_shot.bond_height = env.block.height;
    POOL_ERA_SHOT.save(deps.storage, payload.pool_addr, &pool_era_shot)?;

    Ok(Response::new())
}

pub fn sudo_era_collect_withdraw_failed_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> StdResult<Response> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;
    pool_info.era_process_status = BondEnded;
    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;

    Ok(Response::new())
}

use crate::helper::{check_ibc_fee, gen_msg_send, get_withdraw_ica_id};
use crate::query::query_balance_by_addr;
use crate::state::EraStatus::{BondEnded, WithdrawEnded, WithdrawStarted};
use crate::state::{SudoPayload, TxType, INFO_OF_ICA_ID, POOLS};
use crate::tx_callback::msg_with_sudo_callback;
use crate::{error_conversion::ContractError, helper::DEFAULT_TIMEOUT_SECONDS};
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, Uint128};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronError, NeutronResult,
};

pub fn execute_era_collect_withdraw(
    mut deps: DepsMut<NeutronQuery>,
    info: MessageInfo,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    // check era state
    if pool_info.status != BondEnded {
        return Err(ContractError::StatusNotAllow {}.into());
    }
    pool_info.status = WithdrawStarted;

    let (_, withdraw_ica_info, _) = INFO_OF_ICA_ID.load(deps.storage, pool_info.ica_id.clone())?;

    // check withdraw address balance and send it to the pool
    let withdraw_balances_result: Result<
        neutron_sdk::interchain_queries::v045::queries::BalanceResponse,
        NeutronError,
    > = query_balance_by_addr(deps.as_ref(), withdraw_ica_info.ica_addr.clone());

    let mut withdraw_amount = Uint128::zero();
    if let Ok(balance_response) = withdraw_balances_result {
        if balance_response.last_submitted_local_height <= pool_info.era_snapshot.last_step_height {
            return Err(ContractError::WithdrawAddrBalanceSubmissionHeight {}.into());
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

    // leave gas
    if withdraw_amount.is_zero() {
        pool_info.status = WithdrawEnded;
        POOLS.save(deps.storage, pool_addr.clone(), &pool_info)?;

        return Ok(Response::default());
    }

    let ibc_fee = check_ibc_fee(deps.as_ref(), &info)?;
    let cosmos_msg = NeutronMsg::submit_tx(
        withdraw_ica_info.ctrl_connection_id.clone(),
        get_withdraw_ica_id(pool_info.ica_id.clone()),
        vec![gen_msg_send(
            withdraw_ica_info.ica_addr.clone(),
            pool_addr.clone(),
            pool_info.remote_denom.clone(),
            withdraw_amount.to_string(),
        )?],
        "".to_string(),
        DEFAULT_TIMEOUT_SECONDS,
        ibc_fee,
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

    pool_info.era_snapshot.restake_amount = withdraw_amount;
    POOLS.save(deps.storage, pool_addr, &pool_info)?;

    Ok(Response::default().add_submessage(submsg))
}

pub fn sudo_era_collect_withdraw_callback(
    deps: DepsMut,
    env: Env,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;
    pool_info.status = WithdrawEnded;
    pool_info.era_snapshot.last_step_height = env.block.height;
    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;

    Ok(Response::new())
}

pub fn sudo_era_collect_withdraw_failed_callback(
    deps: DepsMut,
    payload: SudoPayload,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr.clone())?;
    pool_info.status = BondEnded;
    POOLS.save(deps.storage, payload.pool_addr.clone(), &pool_info)?;

    Ok(Response::new())
}

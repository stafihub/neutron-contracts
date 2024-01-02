use cosmwasm_std::{coin, DepsMut, Env, Response, StdError, StdResult, Uint128};
use neutron_sdk::interchain_txs::helpers::get_port_id;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    query::min_ibc_fee::query_min_ibc_fee,
    sudo::msg::RequestPacketTimeoutHeight,
    NeutronError, NeutronResult,
};

use crate::helper::min_ntrn_ibc_fee;
use crate::state::PoolBondState::{ActiveReported, EraUpdated};
use crate::state::{ADDR_ICAID_MAP, POOLS};
use crate::{
    contract::{msg_with_sudo_callback, SudoPayload, TxType},
    state::{EraShot, POOL_ERA_SHOT},
};

pub fn execute_era_update(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    channel: String,
    pool_addr: String,
) -> NeutronResult<Response<NeutronMsg>> {
    let mut pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    // check era state
    if pool_info.era_update_status != ActiveReported {
        deps.as_ref()
            .api
            .debug(format!("WASMDEBUG: execute_era_update skip pool: {:?}", pool_addr).as_str());
        return Err(NeutronError::Std(StdError::generic_err("status not allow")));
    }

    if let Some(pool_era_shot) = POOL_ERA_SHOT.may_load(deps.storage, pool_addr.clone())? {
        if pool_era_shot.failed_tx.is_some()
            && pool_era_shot.failed_tx != Some(TxType::EraUpdateIbcSend)
        {
            return Ok(Response::new());
        }
    } else {
        POOL_ERA_SHOT.save(
            deps.storage,
            pool_addr.clone(),
            &(EraShot {
                pool_addr: pool_addr.clone(),
                era: pool_info.era,
                bond: pool_info.bond,
                unbond: pool_info.unbond,
                active: pool_info.active,
                bond_height: 0,
                failed_tx: None,
            }),
        )?;
    }

    if pool_info.bond.is_zero() {
        pool_info.era_update_status = EraUpdated;
        POOLS.save(deps.storage, pool_info.pool_addr.clone(), &pool_info)?;
        return Ok(Response::default());
    }

    // See more info here: https://docs.neutron.org/neutron/feerefunder/overview
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);

    let balance = deps.querier.query_all_balances(&env.contract.address)?;

    // funds use contract funds
    let mut amount = 0;
    if !balance.is_empty() {
        amount = u128::from(
            balance
                .iter()
                .find(|c| c.denom == pool_info.ibc_denom.clone())
                .map(|c| c.amount)
                .unwrap_or(Uint128::zero()),
        );
    }

    let tx_coin = coin(amount, pool_info.ibc_denom.clone());

    let msg: NeutronMsg = NeutronMsg::IbcTransfer {
        source_port: "transfer".to_string(),
        source_channel: channel.clone(),
        sender: env.contract.address.to_string(),
        receiver: pool_addr.clone(),
        token: tx_coin,
        timeout_height: RequestPacketTimeoutHeight {
            // todo: revision_number from param?
            revision_number: Some(2),
            revision_height: Some(crate::contract::DEFAULT_TIMEOUT_HEIGHT),
        },
        timeout_timestamp: 0,
        memo: "".to_string(),
        fee: fee.clone(),
    };

    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: IbcTransfer msg: {:?}", msg).as_str());

    let interchain_account_id = ADDR_ICAID_MAP.load(deps.storage, pool_addr.clone())?;

    let submsg_pool_ibc_send = msg_with_sudo_callback(
        deps.branch(),
        msg,
        SudoPayload {
            port_id: get_port_id(
                env.contract.address.to_string(),
                interchain_account_id.clone(),
            ),
            pool_addr: pool_addr.clone(),
            message: "".to_string(),
            tx_type: TxType::EraUpdateIbcSend,
        },
    )?;

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: execute_send: sent submsg: {:?}",
            submsg_pool_ibc_send
        )
        .as_str(),
    );

    Ok(Response::default().add_submessage(submsg_pool_ibc_send))
}

pub fn sudo_era_update_callback(deps: DepsMut, payload: SudoPayload) -> StdResult<Response> {
    let mut pool_info = POOLS.load(deps.storage, payload.pool_addr)?;
    pool_info.era_update_status = EraUpdated;
    POOLS.save(deps.storage, pool_info.pool_addr.clone(), &pool_info)?;
    Ok(Response::new())
}

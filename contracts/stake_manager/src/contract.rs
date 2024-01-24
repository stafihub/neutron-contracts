use std::env;

use cosmwasm_std::{
    entry_point, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response,
    StdResult, Uint128,
};
use cw2::set_contract_version;

use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    interchain_queries::get_registered_query,
    NeutronResult,
};
use neutron_sdk::{query::min_ibc_fee::query_min_ibc_fee, sudo::msg::SudoMsg};

use crate::execute_era_rebond::execute_era_rebond;
use crate::execute_era_update::execute_era_update;
use crate::execute_icq_update_period::update_icq_update_period;
use crate::execute_init_pool::execute_init_pool;
use crate::execute_migrate_pool::execute_migrate_pool;
use crate::execute_open_channel::execute_open_channel;
use crate::execute_pool_add_validator::execute_add_pool_validators;
use crate::execute_pool_rm_validator::execute_rm_pool_validator;
use crate::execute_pool_update_validator::execute_pool_update_validator;
use crate::execute_redeem_token_for_share::execute_redeem_token_for_share;
use crate::execute_register_pool::{execute_register_pool, sudo_open_ack};
use crate::execute_stake::execute_stake;
use crate::execute_stake_lsm::execute_stake_lsm;
use crate::execute_unstake::execute_unstake;
use crate::execute_withdraw::execute_withdraw;
use crate::helper::{
    QUERY_REPLY_ID_RANGE_END, QUERY_REPLY_ID_RANGE_START, REPLY_ID_RANGE_END, REPLY_ID_RANGE_START,
};
use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::query::query_delegation_by_addr;
use crate::query::query_era_snapshot;
use crate::query::query_stack_info;
use crate::query::query_user_unstake_index;
use crate::query::{query_balance_by_addr, query_validator_by_addr};
use crate::query::{
    query_interchain_address, query_interchain_address_contract, query_pool_info,
    query_user_unstake,
};
use crate::query_callback::write_reply_id_to_query_id;
use crate::state::{Stack, STACK};
use crate::tx_callback::{prepare_sudo_payload, sudo_error, sudo_response, sudo_timeout};
use crate::{error_conversion::ContractError, query_callback::sudo_kv_query_result};
use crate::{execute_config_pool::execute_config_pool, query::get_ica_registered_query};
use crate::{
    execute_config_stack::execute_config_stack,
    execute_update_validators_icq::execute_update_validators_icq,
};
use crate::{
    execute_era_active::execute_era_active,
    state::{self, POOLS},
};
use crate::{execute_era_bond::execute_era_bond, helper::gen_delegation_txs};
use crate::{
    execute_era_collect_withdraw::execute_era_collect_withdraw,
    helper::{min_ntrn_ibc_fee, DEFAULT_TIMEOUT_SECONDS},
    state::{SudoPayload, TxType, INFO_OF_ICA_ID},
    tx_callback::msg_with_sudo_callback,
};

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> NeutronResult<Response<NeutronMsg>> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    STACK.save(
        deps.storage,
        &(Stack {
            admin: info.sender.clone(),
            stack_fee_receiver: info.sender.clone(),
            stack_fee_commission: Uint128::new(100_000),
            total_stack_fee: Uint128::zero(),
            pools: vec![],
            lsd_token_code_id: msg.lsd_token_code_id,
        }),
    )?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<NeutronQuery>, env: Env, msg: QueryMsg) -> NeutronResult<Binary> {
    match msg {
        QueryMsg::GetRegisteredQuery { query_id } => {
            Ok(to_json_binary(&get_registered_query(deps, query_id)?)?)
        }
        QueryMsg::GetIcaRegisteredQuery {
            ica_addr,
            query_kind,
        } => Ok(to_json_binary(&get_ica_registered_query(
            deps, ica_addr, query_kind,
        )?)?),
        QueryMsg::Balance { ica_addr } => {
            Ok(to_json_binary(&query_balance_by_addr(deps, ica_addr)?)?)
        }
        QueryMsg::Delegations { pool_addr } => {
            Ok(to_json_binary(&query_delegation_by_addr(deps, pool_addr)?)?)
        }
        QueryMsg::Validators { pool_addr } => {
            Ok(to_json_binary(&query_validator_by_addr(deps, pool_addr)?)?)
        }
        QueryMsg::PoolInfo { pool_addr } => query_pool_info(deps, env, pool_addr),
        QueryMsg::StackInfo {} => query_stack_info(deps),
        QueryMsg::EraSnapshot { pool_addr } => query_era_snapshot(deps, env, pool_addr),
        QueryMsg::InterchainAccountAddress {
            interchain_account_id,
            connection_id,
        } => query_interchain_address(deps, env, interchain_account_id, connection_id),
        QueryMsg::InterchainAccountAddressFromContract {
            interchain_account_id,
        } => query_interchain_address_contract(deps, env, interchain_account_id),
        QueryMsg::UserUnstake {
            pool_addr,
            user_neutron_addr,
        } => query_user_unstake(deps, pool_addr, user_neutron_addr),
        QueryMsg::UserUnstakeIndex {
            pool_addr,
            user_neutron_addr,
        } => query_user_unstake_index(deps, pool_addr, user_neutron_addr),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> NeutronResult<Response<NeutronMsg>> {
    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: execute msg is {:?},info is:{:?}", msg, info).as_str());
    match msg {
        ExecuteMsg::RegisterPool {
            connection_id,
            interchain_account_id,
            register_fee,
        } => execute_register_pool(
            deps,
            env,
            info,
            connection_id,
            interchain_account_id,
            register_fee,
        ),
        ExecuteMsg::InitPool(params) => execute_init_pool(deps, env, info, *params),
        ExecuteMsg::MigratePool(params) => execute_migrate_pool(deps, env, info, *params),
        ExecuteMsg::ConfigPool(params) => execute_config_pool(deps, info, *params),
        ExecuteMsg::ConfigStack(params) => execute_config_stack(deps, info, *params),
        ExecuteMsg::OpenChannel {
            pool_addr,
            closed_channel_id,
            register_fee,
        } => execute_open_channel(deps, env, info, pool_addr, closed_channel_id, register_fee),
        ExecuteMsg::RedeemTokenForShare { pool_addr, tokens } => {
            execute_redeem_token_for_share(deps, info, pool_addr, tokens)
        }
        ExecuteMsg::Stake {
            neutron_address,
            pool_addr,
        } => execute_stake(deps, env, neutron_address, pool_addr, info),
        ExecuteMsg::Unstake { amount, pool_addr } => execute_unstake(deps, info, amount, pool_addr),
        ExecuteMsg::Withdraw {
            pool_addr,
            receiver,
            unstake_index_list,
        } => execute_withdraw(deps, env, info, pool_addr, receiver, unstake_index_list),
        ExecuteMsg::PoolRmValidator {
            pool_addr,
            validator_addr,
        } => execute_rm_pool_validator(deps, env, info, pool_addr, validator_addr),
        ExecuteMsg::PoolAddValidator {
            pool_addr,
            validator_addr,
        } => execute_add_pool_validators(deps, env, info, pool_addr, validator_addr),
        ExecuteMsg::PoolUpdateValidator {
            pool_addr,
            old_validator,
            new_validator,
        } => {
            execute_pool_update_validator(deps, env, info, pool_addr, old_validator, new_validator)
        }
        ExecuteMsg::PoolUpdateValidatorsIcq { pool_addr } => {
            execute_update_validators_icq(deps, env, info, pool_addr)
        }
        ExecuteMsg::EraUpdate { pool_addr } => execute_era_update(deps, env, pool_addr),
        ExecuteMsg::EraBond { pool_addr } => execute_era_bond(deps, env, pool_addr),
        ExecuteMsg::PoolBond { pool_addr, amount } => execute_pool_bond(deps, info,pool_addr, amount),
        ExecuteMsg::EraCollectWithdraw { pool_addr } => {
            execute_era_collect_withdraw(deps, env, pool_addr)
        }
        ExecuteMsg::EraRebond { pool_addr } => execute_era_rebond(deps, env, pool_addr),
        ExecuteMsg::EraActive { pool_addr } => execute_era_active(deps, pool_addr),
        ExecuteMsg::StakeLsm {
            neutron_address,
            pool_addr,
        } => execute_stake_lsm(deps, env, info, neutron_address, pool_addr),
        ExecuteMsg::UpdateIcqUpdatePeriod {
            pool_addr,
            new_update_period,
        } => update_icq_update_period(deps, info, pool_addr, new_update_period),
    }
}

fn execute_pool_bond(
    mut deps: DepsMut<NeutronQuery>,
    info: MessageInfo,
    pool_addr: String,
    amount: Uint128,
) -> NeutronResult<Response<NeutronMsg>> {
    let pool_info = POOLS.load(deps.storage, pool_addr.clone())?;

    // check era state
    if pool_info.status != state::EraStatus::ActiveEnded {
        return Err(ContractError::StatusNotAllow {}.into());
    }
    if info.sender != pool_info.admin {
        return Err(ContractError::Unauthorized {}.into());
    }

    let mut msgs = vec![];

    let validator_count = pool_info.validator_addrs.len() as u128;
    if validator_count == 0 {
        return Err(ContractError::ValidatorsEmpty {}.into());
    }

    let any_msg = gen_delegation_txs(
        pool_addr.clone(),
        pool_info.validator_addrs.get(0).unwrap().to_string(),
        pool_info.remote_denom.clone(),
        amount,
    );

    msgs.push(any_msg);

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
            message: "".to_string(),
            pool_addr: pool_addr.clone(),
            tx_type: TxType::AddValidator,// temporary use because there are no callbacks
        },
    )?;

    Ok(Response::default().add_submessage(submsg))
}

#[entry_point]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> StdResult<Response> {
    match msg.id {
        // It's convenient to use range of ID's to handle multiple reply messages
        REPLY_ID_RANGE_START..=REPLY_ID_RANGE_END => prepare_sudo_payload(deps, env, msg),
        QUERY_REPLY_ID_RANGE_START..=QUERY_REPLY_ID_RANGE_END => {
            write_reply_id_to_query_id(deps, msg)
        }

        _ => Err(ContractError::UnsupportedReplyId(msg.id).into()),
    }
}

#[entry_point]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> NeutronResult<Response<NeutronMsg>> {
    match msg {
        // For handling kv query result
        // For handling successful (non-error) acknowledgements
        SudoMsg::Response { request, data } => sudo_response(deps, env, request, data),

        // For handling error acknowledgements
        SudoMsg::Error { request, details } => sudo_error(deps, request, details),

        // For handling error timeouts
        SudoMsg::Timeout { request } => sudo_timeout(deps, request),

        SudoMsg::KVQueryResult { query_id } => sudo_kv_query_result(deps, query_id),

        // For handling successful registering of ICA
        SudoMsg::OpenAck {
            port_id,
            channel_id,
            counterparty_channel_id,
            counterparty_version,
        } => sudo_open_ack(
            deps,
            env,
            port_id,
            channel_id,
            counterparty_channel_id,
            counterparty_version,
        ),

        _ => Ok(Response::default()),
    }
}

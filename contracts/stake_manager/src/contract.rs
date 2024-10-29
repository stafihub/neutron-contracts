use crate::execute_era_active::execute_era_active;
use crate::execute_era_bond::execute_era_bond;
use crate::execute_era_collect_withdraw::execute_era_collect_withdraw;
use crate::execute_era_rebond::execute_era_rebond;
use crate::execute_era_update::execute_era_update;
use crate::execute_icq_update_period::update_icq_update_period;
use crate::execute_init_pool::execute_init_pool;
use crate::execute_migrate_pool::execute_migrate_pool;
use crate::execute_open_channel::execute_open_channel;
use crate::execute_pool_add_validator::execute_add_pool_validators;
use crate::execute_pool_delegate::execute_pool_delegate;
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
use crate::query::query_user_unstake_index;
use crate::query::{query_balance_by_addr, query_validator_by_addr};
use crate::query::{query_delegation_by_addr, query_era_rate};
use crate::query::{query_era_snapshot, query_ids};
use crate::query::{
    query_interchain_address, query_interchain_address_contract, query_pool_info,
    query_user_unstake,
};
use crate::query::{query_stack_info, query_total_stack_fee};
use crate::query_callback::write_reply_id_to_query_id;
use crate::state::{Stack, POOLS, STACK};
use crate::tx_callback::{prepare_sudo_payload, sudo_error, sudo_response, sudo_timeout};
use crate::{error_conversion::ContractError, query_callback::sudo_kv_query_result};
use crate::{execute_config_pool::execute_config_pool, query::get_ica_registered_query};
use crate::{
    execute_config_stack::execute_config_stack,
    execute_update_validators_icq::execute_update_validators_icq,
};
use cosmwasm_std::{
    entry_point, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response,
    StdResult, Uint128,
};
use cw2::set_contract_version;
use neutron_sdk::sudo::msg::SudoMsg;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    interchain_queries::get_registered_query,
    NeutronResult,
};
use std::env;

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
            pools: vec![],
            lsd_token_code_id: msg.lsd_token_code_id,
        }),
    )?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let mut pool_info = POOLS.load(
        deps.storage,
        "cosmos1u9xc9rjxm6epprxzrxqlnj4q95rwac6dupp9kmk8p7l28q6fcsgqm20kxh".to_string(),
    )?;
    pool_info.bond -= pool_info.era_snapshot.bond;
    pool_info.unbond -= pool_info.era_snapshot.unbond;

    POOLS.save(
        deps.storage,
        "cosmos1u9xc9rjxm6epprxzrxqlnj4q95rwac6dupp9kmk8p7l28q6fcsgqm20kxh".to_string(),
        &pool_info,
    )?;

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
        QueryMsg::Delegations {
            pool_addr,
            sdk_greater_or_equal_v047,
        } => Ok(to_json_binary(&query_delegation_by_addr(
            deps,
            pool_addr,
            sdk_greater_or_equal_v047,
        )?)?),
        QueryMsg::Validators { pool_addr } => {
            Ok(to_json_binary(&query_validator_by_addr(deps, pool_addr)?)?)
        }
        QueryMsg::PoolInfo { pool_addr } => query_pool_info(deps, env, pool_addr),
        QueryMsg::StackInfo {} => query_stack_info(deps),
        QueryMsg::TotalStackFee { pool_addr } => query_total_stack_fee(deps, pool_addr),
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
        QueryMsg::EraRate { pool_addr, era } => query_era_rate(deps, pool_addr, era),
        QueryMsg::QueryIds { pool_addr } => query_ids(deps, pool_addr),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> NeutronResult<Response<NeutronMsg>> {
    match msg {
        ExecuteMsg::RegisterPool {
            connection_id,
            interchain_account_id,
        } => execute_register_pool(deps, env, info, connection_id, interchain_account_id),
        ExecuteMsg::InitPool(params) => execute_init_pool(deps, env, info, *params),
        ExecuteMsg::MigratePool(params) => execute_migrate_pool(deps, env, info, *params),
        ExecuteMsg::ConfigPool(params) => execute_config_pool(deps, info, env, *params),
        ExecuteMsg::ConfigStack(params) => execute_config_stack(deps, info, *params),
        ExecuteMsg::OpenChannel {
            pool_addr,
            closed_channel_id,
        } => execute_open_channel(deps, env, info, pool_addr, closed_channel_id),
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
        } => execute_withdraw(deps, info, pool_addr, receiver, unstake_index_list),
        ExecuteMsg::PoolRmValidator {
            pool_addr,
            validator_addr,
        } => execute_rm_pool_validator(deps, info, pool_addr, validator_addr),
        ExecuteMsg::PoolAddValidator {
            pool_addr,
            validator_addr,
        } => execute_add_pool_validators(deps, env, info, pool_addr, validator_addr),
        ExecuteMsg::PoolUpdateValidator {
            pool_addr,
            old_validator,
            new_validator,
        } => execute_pool_update_validator(deps, info, pool_addr, old_validator, new_validator),
        ExecuteMsg::PoolUpdateValidatorsIcq { pool_addr } => {
            execute_update_validators_icq(deps, env, info, pool_addr)
        }
        ExecuteMsg::PoolDelegate {
            pool_addr,
            stake_amount,
        } => execute_pool_delegate(deps, info, pool_addr, stake_amount),
        ExecuteMsg::EraUpdate { pool_addr } => execute_era_update(deps, env, info, pool_addr),
        ExecuteMsg::EraBond { pool_addr } => execute_era_bond(deps, env, info, pool_addr),
        ExecuteMsg::EraCollectWithdraw { pool_addr } => {
            execute_era_collect_withdraw(deps, info, pool_addr)
        }
        ExecuteMsg::EraRebond { pool_addr } => execute_era_rebond(deps, info, pool_addr),
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

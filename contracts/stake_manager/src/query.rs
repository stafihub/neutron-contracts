use crate::state::{QueryKind, INFO_OF_ICA_ID};
use crate::state::{ADDRESS_TO_REPLY_ID, STACK};
use crate::state::{POOLS, REPLY_ID_TO_QUERY_ID, UNSTAKES_INDEX_FOR_USER, UNSTAKES_OF_INDEX};
use cosmwasm_std::{to_json_binary, Addr, Binary, Deps, Env};
use neutron_sdk::{
    bindings::query::QueryRegisteredQueryResponse,
    interchain_queries::v045::queries::{DelegatorDelegationsResponse, ValidatorResponse},
};
use neutron_sdk::{
    bindings::query::{NeutronQuery, QueryInterchainAccountAddressResponse},
    interchain_queries::{
        check_query_type, get_registered_query, query_kv_result,
        types::QueryType,
        v045::{
            queries::BalanceResponse, types::Balances, types::Delegations, types::StakingValidator,
        },
    },
    NeutronResult,
};
use std::vec;

pub fn query_user_unstake(
    deps: Deps<NeutronQuery>,
    pool_addr: String,
    user_neutron_addr: Addr,
) -> NeutronResult<Binary> {
    let mut results = vec![];

    if let Some(unstakes) =
        UNSTAKES_INDEX_FOR_USER.may_load(deps.storage, (user_neutron_addr, pool_addr.clone()))?
    {
        for unstake_index in unstakes {
            let unstake_info =
                UNSTAKES_OF_INDEX.load(deps.storage, (pool_addr.clone(), unstake_index))?;
            results.push(unstake_info);
        }
    }

    Ok(to_json_binary(&results)?)
}

pub fn query_user_unstake_index(
    deps: Deps<NeutronQuery>,
    pool_addr: String,
    user_neutron_addr: Addr,
) -> NeutronResult<Binary> {
    Ok(to_json_binary(
        &UNSTAKES_INDEX_FOR_USER.may_load(deps.storage, (user_neutron_addr, pool_addr))?,
    )?)
}

pub fn query_balance_by_addr(
    deps: Deps<NeutronQuery>,
    addr: String,
) -> NeutronResult<BalanceResponse> {
    let contract_query_id =
        ADDRESS_TO_REPLY_ID.load(deps.storage, (addr, QueryKind::Balances.to_string()))?;
    let registered_query_id = REPLY_ID_TO_QUERY_ID.load(deps.storage, contract_query_id)?;
    // get info about the query
    let registered_query = get_registered_query(deps, registered_query_id)?;

    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Balances structure from raw KV-storage values
    let balances: Balances = query_kv_result(deps, registered_query_id)?;

    Ok(BalanceResponse {
        // last_submitted_height tells us when the query result was updated last time (block height)
        last_submitted_local_height: registered_query
            .registered_query
            .last_submitted_result_local_height,
        balances,
    })
}

pub fn query_delegation_by_addr(
    deps: Deps<NeutronQuery>,
    addr: String,
) -> NeutronResult<DelegatorDelegationsResponse> {
    let contract_query_id =
        ADDRESS_TO_REPLY_ID.load(deps.storage, (addr, QueryKind::Delegations.to_string()))?;
    let registered_query_id = REPLY_ID_TO_QUERY_ID.load(deps.storage, contract_query_id)?;
    // get info about the query
    let registered_query: neutron_sdk::bindings::query::QueryRegisteredQueryResponse =
        get_registered_query(deps, registered_query_id)?;

    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Balances structure from raw KV-storage values
    let delegations: Delegations = query_kv_result(deps, registered_query_id)?;

    Ok(DelegatorDelegationsResponse {
        // last_submitted_height tells us when the query result was updated last time (block height)
        last_submitted_local_height: registered_query
            .registered_query
            .last_submitted_result_local_height,
        delegations: delegations.delegations,
    })
}

pub fn query_validator_by_addr(
    deps: Deps<NeutronQuery>,
    addr: String,
) -> NeutronResult<ValidatorResponse> {
    let contract_query_id =
        ADDRESS_TO_REPLY_ID.load(deps.storage, (addr, QueryKind::Validators.to_string()))?;
    let registered_query_id = REPLY_ID_TO_QUERY_ID.load(deps.storage, contract_query_id)?;
    // get info about the query
    let registered_query: neutron_sdk::bindings::query::QueryRegisteredQueryResponse =
        get_registered_query(deps, registered_query_id)?;

    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Balances structure from raw KV-storage values
    let staking_validator: StakingValidator = query_kv_result(deps, registered_query_id)?;

    Ok(ValidatorResponse {
        // last_submitted_height tells us when the query result was updated last time (block height)
        last_submitted_local_height: registered_query
            .registered_query
            .last_submitted_result_local_height,
        validator: staking_validator,
    })
}

pub fn query_pool_info(
    deps: Deps<NeutronQuery>,
    _env: Env,
    pool_addr: String,
) -> NeutronResult<Binary> {
    let pool_info = POOLS.load(deps.storage, pool_addr)?;

    Ok(to_json_binary(&pool_info)?)
}

pub fn query_stack_info(deps: Deps<NeutronQuery>) -> NeutronResult<Binary> {
    let stack_info = STACK.load(deps.storage)?;

    Ok(to_json_binary(&stack_info)?)
}

pub fn query_era_snapshot(
    deps: Deps<NeutronQuery>,
    _env: Env,
    pool_addr: String,
) -> NeutronResult<Binary> {
    let pool_info = POOLS.load(deps.storage, pool_addr)?;
    let result = pool_info.era_snapshot;

    Ok(to_json_binary(&result)?)
}

// returns ICA address from Neutron ICA SDK module
pub fn query_interchain_address(
    deps: Deps<NeutronQuery>,
    env: Env,
    interchain_account_id: String,
    connection_id: String,
) -> NeutronResult<Binary> {
    let query = NeutronQuery::InterchainAccountAddress {
        owner_address: env.contract.address.to_string(),
        interchain_account_id,
        connection_id,
    };

    let res: QueryInterchainAccountAddressResponse = deps.querier.query(&query.into())?;
    Ok(to_json_binary(&res)?)
}

// returns ICA address from the contract storage. The address was saved in sudo_open_ack method
pub fn query_interchain_address_contract(
    deps: Deps<NeutronQuery>,
    _: Env,
    interchain_account_id: String,
) -> NeutronResult<Binary> {
    let res = INFO_OF_ICA_ID.may_load(deps.storage, interchain_account_id)?;
    Ok(to_json_binary(&res)?)
}

/// Queries registered query info by ica address and query kind
pub fn get_ica_registered_query(
    deps: Deps<NeutronQuery>,
    ica_addr: String,
    query_kind: QueryKind,
) -> NeutronResult<QueryRegisteredQueryResponse> {
    let contract_query_id =
        ADDRESS_TO_REPLY_ID.load(deps.storage, (ica_addr, query_kind.to_string()))?;
    let registered_query_id = REPLY_ID_TO_QUERY_ID.load(deps.storage, contract_query_id)?;

    let query = NeutronQuery::RegisteredInterchainQuery {
        query_id: registered_query_id,
    };

    let res: QueryRegisteredQueryResponse = deps.querier.query(&query.into())?;
    Ok(res)
}

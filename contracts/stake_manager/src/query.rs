use std::vec;

use crate::helper::get_ica;
use cosmwasm_std::{to_json_binary, Addr, Binary, Deps, Env};
use neutron_sdk::{
    bindings::query::{NeutronQuery, QueryInterchainAccountAddressResponse},
    interchain_queries::{
        check_query_type, get_registered_query, query_kv_result,
        types::QueryType,
        v045::{queries::BalanceResponse, types::Balances, types::Delegations},
    },
    NeutronResult,
};
use neutron_sdk::{
    interchain_queries::v045::queries::DelegatorDelegationsResponse,
    interchain_txs::helpers::get_port_id,
};

use crate::state::{read_errors_from_queue, ACKNOWLEDGEMENT_RESULTS, ADDR_QUERY_ID};
use crate::state::{OWN_QUERY_ID_TO_ICQ_ID, POOLS, UNSTAKES_INDEX_FOR_USER, UNSTAKES_OF_INDEX};

pub fn query_user_unstake(
    deps: Deps<NeutronQuery>,
    pool_addr: String,
    user_neutron_addr: Addr,
) -> NeutronResult<Binary> {
    let mut results = vec![];

    if let Some(unstakes) = UNSTAKES_INDEX_FOR_USER.may_load(deps.storage, &user_neutron_addr)? {
        for (unstake_pool, unstake_index) in unstakes.into_iter().flatten() {
            if unstake_pool != pool_addr {
                continue;
            }
            let unstake_info = UNSTAKES_OF_INDEX.load(deps.storage, unstake_index)?;
            results.push(unstake_info);
        }
    }

    Ok(to_json_binary(&results)?)
}

pub fn query_balance_by_addr(
    deps: Deps<NeutronQuery>,
    addr: String,
) -> NeutronResult<BalanceResponse> {
    let contract_query_id = ADDR_QUERY_ID.load(deps.storage, addr)?;
    let registered_query_id = OWN_QUERY_ID_TO_ICQ_ID.load(deps.storage, contract_query_id)?;
    // get info about the query
    let registered_query = get_registered_query(deps, registered_query_id)?;
    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Balances structure from raw KV-storage values
    let balances: Balances = query_kv_result(deps, registered_query_id)?;

    deps.api.debug(
        format!(
            "WASMDEBUG: query_balance_by_addr Balances is {:?}",
            balances
        )
        .as_str(),
    );

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
    let contract_query_id = ADDR_QUERY_ID.load(deps.storage, addr)?;
    let registered_query_id = OWN_QUERY_ID_TO_ICQ_ID.load(deps.storage, contract_query_id)?;
    // get info about the query
    let registered_query = get_registered_query(deps, registered_query_id)?;
    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Balances structure from raw KV-storage values
    let delegations: Delegations = query_kv_result(deps, registered_query_id)?;

    deps.api.debug(
        format!(
            "WASMDEBUG: query_delegation_by_addr Delegations is {:?}",
            delegations
        )
        .as_str(),
    );

    Ok(DelegatorDelegationsResponse {
        // last_submitted_height tells us when the query result was updated last time (block height)
        last_submitted_local_height: registered_query
            .registered_query
            .last_submitted_result_local_height,
        delegations: delegations.delegations,
    })
}

pub fn query_delegation(
    deps: Deps<NeutronQuery>,
    registered_query_id: u64,
) -> NeutronResult<DelegatorDelegationsResponse> {
    // get info about the query
    let registered_query = get_registered_query(deps, registered_query_id)?;
    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Balances structure from raw KV-storage values
    let delegations: Delegations = query_kv_result(deps, registered_query_id)?;

    deps.api.debug(
        format!(
            "WASMDEBUG: query_delegation_by_addr Delegations is {:?}",
            delegations
        )
        .as_str(),
    );

    Ok(DelegatorDelegationsResponse {
        // last_submitted_height tells us when the query result was updated last time (block height)
        last_submitted_local_height: registered_query
            .registered_query
            .last_submitted_result_local_height,
        delegations: delegations.delegations,
    })
}

pub fn query_balance(
    deps: Deps<NeutronQuery>,
    _env: Env,
    registered_query_id: u64,
) -> NeutronResult<Binary> {
    // get info about the query
    let registered_query = get_registered_query(deps, registered_query_id)?;
    // check that query type is KV
    check_query_type(registered_query.registered_query.query_type, QueryType::KV)?;
    // reconstruct a nice Balances structure from raw KV-storage values
    let balances: Balances = query_kv_result(deps, registered_query_id)?;

    deps.api
        .debug(format!("WASMDEBUG: query_balance Balances is {:?}", balances).as_str());

    Ok(to_json_binary(
        &(BalanceResponse {
            // last_submitted_height tells us when the query result was updated last time (block height)
            last_submitted_local_height: registered_query
                .registered_query
                .last_submitted_result_local_height,
            balances,
        }),
    )?)
}

pub fn query_pool_info(
    deps: Deps<NeutronQuery>,
    _env: Env,
    pool_addr: String,
) -> NeutronResult<Binary> {
    let pool_info = POOLS.load(deps.storage, pool_addr)?;

    Ok(to_json_binary(&pool_info)?)
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
    env: Env,
    interchain_account_id: String,
) -> NeutronResult<Binary> {
    Ok(to_json_binary(&get_ica(
        deps,
        &env,
        &interchain_account_id,
    )?)?)
}

// returns the result
pub fn query_acknowledgement_result(
    deps: Deps<NeutronQuery>,
    env: Env,
    interchain_account_id: String,
    sequence_id: u64,
) -> NeutronResult<Binary> {
    let port_id = get_port_id(env.contract.address.as_str(), &interchain_account_id);
    let res = ACKNOWLEDGEMENT_RESULTS.may_load(deps.storage, (port_id, sequence_id))?;
    Ok(to_json_binary(&res)?)
}

pub fn query_errors_queue(deps: Deps<NeutronQuery>) -> NeutronResult<Binary> {
    let res = read_errors_from_queue(deps.storage)?;
    Ok(to_json_binary(&res)?)
}

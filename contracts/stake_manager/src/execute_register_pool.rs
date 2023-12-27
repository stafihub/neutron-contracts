use cosmwasm_std::{Addr, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128};
use neutron_sdk::interchain_txs::helpers::get_port_id;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state::{PoolBondState, PoolInfo, INTERCHAIN_ACCOUNTS, POOLS, POOL_ICA_MAP};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
struct OpenAckVersion {
    version: String,
    controller_connection_id: String,
    host_connection_id: String,
    address: String,
    encoding: String,
    tx_type: String,
}

pub fn execute_register_pool(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    _: MessageInfo,
    connection_id: String,
    interchain_account_id: String,
    register_fee: Vec<cosmwasm_std::Coin>,
) -> NeutronResult<Response<NeutronMsg>> {
    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: register_fee {:?}", register_fee).as_str());
    let register = NeutronMsg::register_interchain_account(
        connection_id.clone(),
        interchain_account_id.clone(),
        Some(register_fee),
    );

    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: register msg is {:?}", register).as_str());

    let key = get_port_id(env.contract.address.as_str(), &interchain_account_id);

    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: register key is {:?}", key).as_str());

    // we are saving empty data here because we handle response of registering ICA in sudo_open_ack method
    INTERCHAIN_ACCOUNTS.save(deps.storage, key, &None)?;

    Ok(Response::default().add_message(register))
}

// handler register pool
pub fn sudo_open_ack(
    deps: DepsMut,
    _env: Env,
    port_id: String,
    _channel_id: String,
    _counterparty_channel_id: String,
    counterparty_version: String,
) -> StdResult<Response> {

    deps.api.debug(format!("WASMDEBUG: sudo_open_ack: sudo received: {:?} {}", port_id, counterparty_version).as_str());


    // The version variable contains a JSON value with multiple fields,
    // including the generated account address.
    let parsed_version: Result<OpenAckVersion, _> =
        serde_json_wasm::from_str(counterparty_version.as_str());

    // Update the storage record associated with the interchain account.
    if let Ok(parsed_version) = parsed_version {
        INTERCHAIN_ACCOUNTS.save(
            deps.storage,
            port_id,
            &Some((
                parsed_version.address.clone(),
                parsed_version.controller_connection_id.clone(),
            )),
        )?;
        POOL_ICA_MAP.save(
            deps.storage,
            parsed_version.address.clone(),
            &parsed_version.controller_connection_id,
        )?;
        let pool_info = PoolInfo {
            need_withdraw: Uint128::zero(),
            unbond: Uint128::zero(),
            active: Uint128::zero(),
            rtoken: Addr::unchecked(""),
            withdraw_addr: "".to_string(),
            pool_addr: parsed_version.address.clone(),
            ibc_denom: "".to_string(),
            remote_denom: "".to_string(),
            connection_id: "".to_string(),
            validator_addrs: vec![],
            era: 0,
            rate: Uint128::zero(),
            minimal_stake: Uint128::zero(),
            unstake_times_limit: Uint128::zero(),
            next_unstake_index: Uint128::zero(),
            unbonding_period: 0,
            era_update_status: PoolBondState::ActiveReported,
            unbond_commission: Uint128::zero(),
            protocol_fee_receiver: Addr::unchecked(""),
        };
        POOLS.save(deps.storage, parsed_version.address.clone(), &pool_info)?;
        return Ok(Response::default());
    }
    Err(StdError::generic_err("Can't parse counterparty_version"))
}

use cosmwasm_std::{Addr, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use neutron_sdk::interchain_txs::helpers::get_port_id;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

use crate::{
    helper::get_ica,
    state::{EraProcessStatus, PoolInfo, ADDR_ICAID_MAP, INTERCHAIN_ACCOUNTS, POOLS},
};

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
        Some(register_fee.clone()),
    );

    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: register msg is {:?}", register).as_str());

    let withdraw_addr_inter_id = format!("{}-withdraw_addr", interchain_account_id.clone());

    let key = get_port_id(env.contract.address.as_str(), &interchain_account_id);

    let register_withdraw = NeutronMsg::register_interchain_account(
        connection_id.clone(),
        withdraw_addr_inter_id.clone(),
        Some(register_fee),
    );

    let key_withdraw = get_port_id(env.contract.address.as_str(), &withdraw_addr_inter_id);

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG: register key is {:?} key_withdraw is {:?} ",
            key, key_withdraw
        )
        .as_str(),
    );

    // we are saving empty data here because we handle response of registering ICA in sudo_open_ack method
    INTERCHAIN_ACCOUNTS.save(deps.storage, key, &None)?;
    INTERCHAIN_ACCOUNTS.save(deps.storage, key_withdraw, &None)?;

    Ok(Response::default().add_messages(vec![register_withdraw, register]))
}

// handler register pool
pub fn sudo_open_ack(
    deps: DepsMut,
    env: Env,
    port_id: String,
    _channel_id: String,
    _counterparty_channel_id: String,
    counterparty_version: String,
) -> StdResult<Response> {
    deps.api.debug(
        format!(
            "WASMDEBUG: sudo_open_ack: sudo received: {:?} {}",
            port_id, counterparty_version
        )
        .as_str(),
    );

    // The version variable contains a JSON value with multiple fields,
    // including the generated account address.
    let parsed_version: Result<OpenAckVersion, _> =
        serde_json_wasm::from_str(counterparty_version.as_str());

    // Update the storage record associated with the interchain account.
    if let Ok(parsed_version) = parsed_version {
        let parts: Vec<String> = port_id.split('.').map(String::from).collect();

        INTERCHAIN_ACCOUNTS.save(
            deps.storage,
            port_id.clone(),
            &Some((
                parsed_version.address.clone(),
                parsed_version.controller_connection_id.clone(),
            )),
        )?;

        ADDR_ICAID_MAP.save(
            deps.storage,
            parsed_version.address.clone(),
            &parts.get(1).unwrap(),
        )?;

        if port_id.contains("withdraw_addr") {
            let ica_parts: Vec<String> =
                parts.get(1).unwrap().split('-').map(String::from).collect();

            let (delegator, _) = get_ica(deps.as_ref(), &env, &ica_parts.first().unwrap().clone())?;

            let mut pool_info = POOLS.load(deps.storage, delegator.clone())?;

            deps.api
                .debug(format!("WASMDEBUG: sudo_open_ack: pool_info: {:?}", pool_info).as_str());

            pool_info.withdraw_addr = parsed_version.address.clone();
            POOLS.save(deps.storage, delegator, &pool_info)?;
        } else {
            let pool_info = PoolInfo {
                bond: Uint128::zero(),
                unbond: Uint128::zero(),
                active: Uint128::zero(),
                rtoken: Addr::unchecked(""),
                withdraw_addr: "".to_string(),
                pool_addr: parsed_version.address.clone(),
                ibc_denom: "".to_string(),
                remote_denom: "".to_string(),
                connection_id: parsed_version.host_connection_id,
                validator_addrs: vec![],
                era: 0,
                rate: Uint128::zero(),
                minimal_stake: Uint128::zero(),
                unstake_times_limit: 0,
                next_unstake_index: 0,
                unbonding_period: 0,
                era_process_status: EraProcessStatus::ActiveEnded,
                unbond_commission: Uint128::zero(),
                protocol_fee_receiver: Addr::unchecked(""),
                admin: Addr::unchecked(""),
                era_seconds: 0,
                offset: 0,
            };
            POOLS.save(deps.storage, parsed_version.address.clone(), &pool_info)?;
        }

        return Ok(Response::default());
    }
    Err(StdError::generic_err("Can't parse counterparty_version"))
}

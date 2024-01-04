use cosmwasm_std::{Addr, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128};
use neutron_sdk::NeutronError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use neutron_sdk::interchain_txs::helpers::get_port_id;
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};

use crate::{
    helper::{get_withdraw_ica_id, ICA_WITHDRAW_SUFIX},
    state::{EraProcessStatus, IcaInfo, PoolInfo, INFO_OF_ICA_ID, POOLS},
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

    if interchain_account_id.contains(".") || interchain_account_id.contains("-") {
        return Err(NeutronError::Std(StdError::generic_err(
            "Invalid interchain_account_id",
        )));
    }

    if INFO_OF_ICA_ID
        .load(deps.storage, interchain_account_id.clone())
        .is_ok()
    {
        return Err(NeutronError::Std(StdError::generic_err(
            "nterchain_account_id already exist",
        )));
    }

    let register_pool_msg = NeutronMsg::register_interchain_account(
        connection_id.clone(),
        interchain_account_id.clone(),
        Some(register_fee.clone()),
    );

    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: register pool msg is {:?}", register_pool_msg).as_str());

    let withdraw_ica_id = get_withdraw_ica_id(interchain_account_id.clone());
    let register_withdraw_msg = NeutronMsg::register_interchain_account(
        connection_id.clone(),
        withdraw_ica_id.clone(),
        Some(register_fee),
    );

    let ctrl_port_id_of_pool = get_port_id(
        env.contract.address.as_str(),
        &interchain_account_id.clone(),
    );
    let ctrl_port_id_of_withdraw = get_port_id(env.contract.address.as_str(), &withdraw_ica_id);

    deps.as_ref().api.debug(
        format!(
            "WASMDEBUG:  pool port is {:?} withdraw port is {:?} ",
            ctrl_port_id_of_pool, ctrl_port_id_of_withdraw
        )
        .as_str(),
    );

    INFO_OF_ICA_ID.save(
        deps.storage,
        interchain_account_id.clone(),
        &(
            IcaInfo {
                ctrl_connection_id: connection_id.clone(),
                host_connection_id: "".to_string(),
                ctrl_channel_id: "".to_string(),
                host_channel_id: "".to_string(),
                ctrl_port_id: ctrl_port_id_of_pool,
                ica_addr: "".to_string(),
            },
            IcaInfo {
                ctrl_connection_id: connection_id.clone(),
                host_connection_id: "".to_string(),
                ctrl_channel_id: "".to_string(),
                host_channel_id: "".to_string(),
                ctrl_port_id: ctrl_port_id_of_withdraw,
                ica_addr: "".to_string(),
            },
        ),
    )?;

    Ok(Response::default().add_messages(vec![register_pool_msg, register_withdraw_msg]))
}

// handler register pool
pub fn sudo_open_ack(
    deps: DepsMut,
    _: Env,
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
    if let Ok(parsed_version) = parsed_version {
        let port_id_parts: Vec<String> = port_id.split('.').map(String::from).collect();
        if port_id_parts.len() != 2 {
            return Err(StdError::generic_err("counterparty_version not match"));
        }

        let ica_id_raw = port_id_parts.get(1).unwrap();
        let mut is_pool = true;
        let ica_id = if ica_id_raw.contains(ICA_WITHDRAW_SUFIX) {
            is_pool = false;
            ica_id_raw
                .strip_suffix(ICA_WITHDRAW_SUFIX)
                .unwrap()
                .to_string()
        } else {
            ica_id_raw.clone()
        };

        let (mut pool_ica_info, mut withdraw_ica_info) =
            INFO_OF_ICA_ID.load(deps.storage, ica_id.clone())?;
        if is_pool {
            pool_ica_info.ctrl_channel_id = _channel_id;
            pool_ica_info.ctrl_port_id = port_id;
            pool_ica_info.host_connection_id = parsed_version.host_connection_id;
            pool_ica_info.host_channel_id = _counterparty_channel_id;
            pool_ica_info.ica_addr = parsed_version.address;
        } else {
            withdraw_ica_info.ctrl_channel_id = _channel_id;
            withdraw_ica_info.ctrl_port_id = port_id;
            withdraw_ica_info.host_connection_id = parsed_version.host_connection_id;
            withdraw_ica_info.host_channel_id = _counterparty_channel_id;
            withdraw_ica_info.ica_addr = parsed_version.address;
        }

        if !pool_ica_info.ica_addr.is_empty() && !withdraw_ica_info.ica_addr.is_empty() {
            let pool_info = PoolInfo {
                bond: Uint128::zero(),
                unbond: Uint128::zero(),
                active: Uint128::zero(),
                rtoken: Addr::unchecked(""),
                ica_id: ica_id.clone(),
                ibc_denom: "".to_string(),
                remote_denom: "".to_string(),
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
            POOLS.save(deps.storage, pool_ica_info.ica_addr.clone(), &pool_info)?;
        }

        INFO_OF_ICA_ID.save(
            deps.storage,
            ica_id.clone(),
            &(pool_ica_info, withdraw_ica_info),
        )?;

        return Ok(Response::default());
    } else {
        Err(StdError::generic_err("Can't parse counterparty_version"))
    }
}

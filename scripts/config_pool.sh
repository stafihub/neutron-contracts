#!/usr/bin/env bash

config_pool() {
  echo "--------------------------register pool-------------------------------------"
  # pion-1 100000
  msg='{"register_pool":{
    "connection_id": "connection-0",
    "interchain_account_id": "test1",
    "register_fee":[
      {
          "denom":"untrn",
          "amount": "1000000"
      }
    ]
  }}'

  tx_result="$(neutrond tx wasm execute "$contract_address" "$msg" \
      --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
      --broadcast-mode=sync --gas-prices 0.0055untrn --gas 2000000 \
      --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"
  # --amount 2000000untrn \

  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to register interchain account: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi

  echo "Waiting 15 seconds for interchain account (sometimes it takes a lot of time)…"

  # shellcheck disable=SC2034
  for i in $(seq 15); do
    sleep 1
    echo -n .
  done
  echo " done"

  query='{"interchain_account_address_from_contract":{"interchain_account_id":"test1"}}'
  echo "info of pool ica id is: "
  neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq
  pool_address=$(neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq '.data[0].ica_addr' | sed 's/\"//g')
  withdraw_addr=$(neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq '.data[1].ica_addr' | sed 's/\"//g')

  echo "ICA(Pool) address: $pool_address"
  echo "withdraw_addr: $withdraw_addr"

  echo "-------------------------- init pool -------------------------------------"

  msg=$(printf '{
    "init_pool": {
      "interchain_account_id": "test1",
      "unbond": "0",
      "active": "0",
      "bond": "0",
      "ibc_denom": "ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2",
      "channel_id_of_ibc_denom": "channel-0",
      "remote_denom": "uatom",
      "validator_addrs": ["cosmosvaloper18hl5c9xn5dze2g50uaw0l2mr02ew57zk0auktn"],
      "era": 1,
      "platform_fee_receiver": "%s",
      "total_platform_fee": "0",
      "rate": "1000000",
      "lsd_token_name": "lsdTokenNameX",
      "lsd_token_symbol": "symbolX",
      "share_tokens": []
    }
  }' "$ADDRESS_1")

  # echo "the msg is: $msg"
  tx_result="$(neutrond tx wasm execute "$contract_address" "$msg" \
      --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
      --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
      --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx
  )"
  # --amount 5000000untrn \

  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to init pool: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi

  echo "Waiting 10 seconds for init pool (sometimes it takes a lot of time)…"
  # shellcheck disable=SC2034
  for i in $(seq 10); do
    sleep 1
    echo -n .
  done
  echo " done"

  echo "------------------------ pool_info after init  ------------------------"
  query="$(printf '{"pool_info": {"pool_addr": "%s"}}' "$pool_address")"
  # echo "$query"
  neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq
  lsd_token_contract_address=$(neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq .data.lsd_token | sed 's/\"//g')
  echo "lsd_token_contract_address: $lsd_token_contract_address"

  echo "-------------------------- config pool -------------------------------------"

  msg=$(printf '{
  "config_pool": {
    "pool_addr": "%s",
    "platform_fee_receiver": "%s",
    "minimal_stake": "1000",
    "unstake_times_limit": 10,
    "unbonding_period": 1,
    "platform_fee_commission": "100000",
    "lsm_support": true,
    "paused": false,
    "rate_change_limit": "500000",
    "lsm_pending_limit": 60,
    "era_seconds": 60,
    "offset": 26657
  }
}' "$pool_address" "$ADDRESS_1")
  # echo "config pool msg is: $msg"
  tx_result="$(neutrond tx wasm execute "$contract_address" "$msg" \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to config pool: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi

  query="$(printf '{"pool_info": {"pool_addr": "%s"}}' "$pool_address")"
  echo "------------------------ pool_info after config ------------------------"
  neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq

  echo "------------------------ DelegatorWithdrawAddress Query -----------------------------------"
  grpcurl -plaintext -d "{\"delegator_address\":\"$pool_address\"}" localhost:9090 cosmos.distribution.v1beta1.Query/DelegatorWithdrawAddress | jq
}

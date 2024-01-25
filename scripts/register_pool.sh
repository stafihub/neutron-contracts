#!/usr/bin/env bash

register_pool() {
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

  echo "-------------------------- transfer uatom through ibc -------------------------------------"

  tx_result=$(gaiad tx ibc-transfer transfer transfer channel-0 \
    "$ADDRESS_1" 1000uatom \
    --gas auto --gas-adjustment 1.4 \
    --fees 1000uatom --from $ADDRESS_2 \
    --keyring-backend=test --home="$HOME_2" \
    --chain-id="$CHAIN_ID_2" --node "$GAIA_NODE" \
    -y --output json | wait_tx_gaia)

  #echo "$tx_result" | jq .
  code="$(echo "$tx_result" | jq '.code')"
  tx_hash="$(echo "$tx_result" | jq '.txhash')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to send ibc hook to contract: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi
  echo "$tx_hash"

  echo "Waiting 10 seconds for ibc relay (sometimes it takes a lot of time)…"
  # shellcheck disable=SC2034
  for i in $(seq 10); do
    sleep 1
    echo -n .
  done
  echo " done"
}

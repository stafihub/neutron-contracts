#!/usr/bin/env bash
init_bridge() {
  echo "--------------------------init bridge-------------------------------------"

  msg=$(printf '{
    "admin": "%s",
    "threshold": 1,
    "relayers": ["%s"]
}' "$ADDRESS_1" "$ADDRESS_1")

  echo "msg is: $msg"

  bridge_contract_address=$(neutrond tx wasm instantiate "$bridge_code_id" "$msg" \
    --amount 300000000untrn \
    --from "$ADDRESS_1" --admin "$ADMIN" -y --chain-id "$CHAIN_ID_1" \
    --output json --broadcast-mode=sync --label "init" \
    --keyring-backend=test --gas-prices 0.0025untrn --gas auto \
    --gas-adjustment 1.4 --home "$HOME_1" \
    --node "$NEUTRON_NODE" 2>/dev/null |
    wait_tx | jq -r '.events[] | select(.type == "instantiate").attributes[] | select(.key == "_contract_address").value')
  echo "Bridge contract address: $bridge_contract_address"

  query="$(printf '{"bridge_info": {}}')"
  echo "bridge info is: "
  echo "$query"
  neutrond query wasm contract-state smart "$bridge_contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq

echo "--------------bridge add_resource_id_to_token---------------------------"

msg=$(printf '{
  "add_resource_id_to_token": {
    "resource_id": "0x0012345678",
    "token": "%s"
  }
}' "$lsd_token_contract_address")
  # echo $msg
  tx_result="$(neutrond tx wasm execute "$bridge_contract_address" "$msg" \
    --amount 300000000untrn \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to set resource id to token: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi



  echo "--------------lsd token add minter---------------------------"
  msg=$(printf '{
  "add_minter": {
    "new_minter": "%s"
  }
}' "$bridge_contract_address")
  echo $msg
  tx_result="$(neutrond tx wasm execute "$lsd_token_contract_address" "$msg" \
    --amount 300000000untrn \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to vote proposal: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi

  echo "--------------bridge vote proposal---------------------------"

  msg=$(printf '{
  "vote_proposal": {
    "chain_id": 1,
    "deposit_nonce": 1,
    "resource_id":"0x0012345678",
    "recipient": "%s",
    "amount": "8888"
  }
}' "$ADDRESS_1")
  # echo $msg
  tx_result="$(neutrond tx wasm execute "$bridge_contract_address" "$msg" \
    --amount 300000000untrn \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to vote proposal: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi

}

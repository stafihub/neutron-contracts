#!/usr/bin/env bash
config_stack() {
  echo "--------------------------instantiate stake manager-------------------------------------"

  contract_address=$(neutrond tx wasm instantiate "$stake_manager_code_id" '{}' \
    --from "$ADDRESS_1" --admin "$ADMIN" -y --chain-id "$CHAIN_ID_1" \
    --output json --broadcast-mode=sync --label "init" \
    --keyring-backend=test --gas-prices 0.0025untrn --gas auto \
    --gas-adjustment 1.4 --home "$HOME_1" \
    --node "$NEUTRON_NODE" 2>/dev/null |
    wait_tx | jq -r '.logs[0].events[] | select(.type == "instantiate").attributes[] | select(.key == "_contract_address").value')
  echo "Contract address: $contract_address"

  echo "--------------Sent money to contract to pay fees---------------------------"

  tx_result="$(neutrond tx bank send demowallet1 "$contract_address" 1000000untrn \
    --chain-id "$CHAIN_ID_1" --home "$HOME_1" --node "$NEUTRON_NODE" \
    --keyring-backend=test -y --gas-prices 0.0025untrn \
    --broadcast-mode=sync --output json | wait_tx)"

  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to send money to contract: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi

  echo "--------------------------config stack-------------------------------------"

  msg=$(printf '{
  "config_stack": {
    "add_operator": "%s"
  }
}' "$ADDRESS_1")

  # echo "the msg is: $msg"
  tx_result="$(neutrond tx wasm execute "$contract_address" "$msg" \
    --amount 2000000untrn \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to config stack: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi

  echo "Waiting 10 seconds for config stack (sometimes it takes a lot of time)…"
  # shellcheck disable=SC2034
  for i in $(seq 10); do
    sleep 1
    echo -n .
  done
  echo " done"

  echo "-------------------------- update lsd token code id-------------------------------------"

  msg=$(printf '{
  "update_lsd_token_code_id": {
    "code_id": %d
  }
}' "$lsd_code_id")

  # echo "the msg is: $msg"

  tx_result="$(neutrond tx wasm execute "$contract_address" "$msg" \
    --amount 2000000untrn \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"
  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to update lsd code id: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi
}

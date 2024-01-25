#!/usr/bin/env bash

redeem_token_for_share() {

  echo "-------------------------- redeem token for share -------------------------------------"

  query="$(printf '{"pool_info": {"pool_addr": "%s"}}' "$pool_address")"
  echo "pool_info after config is: "
  tokens=$(neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq ".data.share_tokens")

  # era_update round 1
  redeem_msg=$(printf '{
  "redeem_token_for_share": {
    "pool_addr": "%s",
    "tokens": %s
  }
}' "$pool_address" "$tokens")

  echo "redeem msg: $redeem_msg"
  tx_result="$(neutrond tx wasm execute "$contract_address" "$redeem_msg" \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to redeem msg: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi

  echo "Waiting 15 seconds for redeem (sometimes it takes a lot of time)…"
  # shellcheck disable=SC2034
  for i in $(seq 15); do
    sleep 1
    echo -n .
  done
  echo " done"

  query="$(printf '{"pool_info": {"pool_addr": "%s"}}' "$pool_address")"
  echo "pool_info is: "
  echo "$query"
  neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq

}

process_era() {
  echo "-------------------------- era update -------------------------------------"
  # era_update round 1
  era_update_msg=$(printf '{
  "era_update": {
    "pool_addr": "%s"
  }
}' "$pool_address")

  tx_result="$(neutrond tx wasm execute "$contract_address" "$era_update_msg" \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to era_update msg: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi

  echo "Waiting 10 seconds for era_update (sometimes it takes a lot of time)…"
  # shellcheck disable=SC2034
  for i in $(seq 10); do
    sleep 1
    echo -n .
  done
  echo " done"

  echo "query ica atom balance"
  gaiad query bank balances "$pool_address" --node "$GAIA_NODE" --output json | jq

  query="$(printf '{"pool_info": {"pool_addr": "%s"}}' "$pool_address")"
  echo "pool_info is: "
  echo "$query"
  neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq
  echo "Waiting 40 seconds"
  # shellcheck disable=SC2034
  for i in $(seq 40); do
    sleep 1
    echo -n .
  done
  echo " done"

  echo "-------------------------- era bond -------------------------------------"
  # era_bond round 1
  bond_msg=$(printf '{
  "era_bond": {
    "pool_addr": "%s"
  }
}' "$pool_address")

  tx_result="$(neutrond tx wasm execute "$contract_address" "$bond_msg" \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to era_bond msg: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi

  echo "Waiting 15 seconds for era_bond (sometimes it takes a lot of time)…"
  # shellcheck disable=SC2034
  for i in $(seq 15); do
    sleep 1
    echo -n .
  done
  echo " done"

  gaiad query staking delegations "$pool_address" --node "$GAIA_NODE" --output json | jq

  gaiad query bank balances "$pool_address" --node "$GAIA_NODE" --output json | jq

  query="$(printf '{"pool_info": {"pool_addr": "%s"}}' "$pool_address")"
  echo "pool_info is: "
  echo "$query"
  neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq

  echo "--------------------------collect withdraw -------------------------------------"
  echo "Waiting 40 seconds"
  # shellcheck disable=SC2034
  for i in $(seq 40); do
    sleep 1
    echo -n .
  done
  echo " done"
  # era_collect_withdraw_msg round 1
  era_collect_withdraw_msg=$(printf '{
  "era_collect_withdraw": {
    "pool_addr": "%s"
  }
}' "$pool_address")

  tx_result="$(neutrond tx wasm execute "$contract_address" "$era_collect_withdraw_msg" \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to era_collect_withdraw_msg msg: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi

  echo "Waiting 10 seconds for era_collect_withdraw_msg (sometimes it takes a lot of time)…"
  # shellcheck disable=SC2034
  for i in $(seq 10); do
    sleep 1
    echo -n .
  done
  echo " done"

  query="$(printf '{"pool_info": {"pool_addr": "%s"}}' "$pool_address")"
  echo "pool_info is: "
  echo "$query"
  neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq

  echo "-------------------------- era rebond -------------------------------------"

  era_restake_msg=$(printf '{
  "era_rebond": {
    "pool_addr": "%s"
  }
}' "$pool_address")

  tx_result="$(neutrond tx wasm execute "$contract_address" "$era_restake_msg" \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to era_restake_msg msg: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi

  echo "Waiting 10 seconds for era_restake_msg (sometimes it takes a lot of time)…"
  # shellcheck disable=SC2034
  for i in $(seq 10); do
    sleep 1
    echo -n .
  done
  echo " done"

  gaiad query bank balances "$pool_address" --node "$GAIA_NODE" --output json | jq

  query="$(printf '{"pool_info": {"pool_addr": "%s"}}' "$pool_address")"
  echo "pool_info is: "
  echo "$query"
  neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq

  echo "--------------------------era active-------------------------------------"
  echo "Waiting 40 seconds"
  # shellcheck disable=SC2034
  for i in $(seq 40); do
    sleep 1
    echo -n .
  done
  echo " done"

  # era_active_msg round 1
  era_active_msg=$(printf '{
  "era_active": {
    "pool_addr": "%s"
  }
}' "$pool_address")

  tx_result="$(neutrond tx wasm execute "$contract_address" "$era_active_msg" \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to era_active_msg msg: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi

  echo "Waiting 10 seconds for era_active_msg (sometimes it takes a lot of time)…"
  # shellcheck disable=SC2034
  for i in $(seq 10); do
    sleep 1
    echo -n .
  done
  echo " done"

  query="$(printf '{"pool_info": {"pool_addr": "%s"}}' "$pool_address")"
  echo "pool_info is: "
  echo "$query"
  neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq

  query="$(printf '{"delegations": {"pool_addr": "%s"}}' "$pool_address")"
  echo "the query is $query"
  neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq

  # withdraw_addr="cosmos10h9stc5v6ntgeygf5xf945njqq5h32r53uquvw"query_id=3
  echo "---------------------------------------------------------------"
}

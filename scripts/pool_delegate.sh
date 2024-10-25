#!/usr/bin/env bash

pool_delegate() {
  gaiad tx bank send "$ADDRESS_2" "$pool_address" 880000$HOSTCHAINDENOM \
        --gas auto --gas-adjustment 1.4 \
        --fees 1000$HOSTCHAINDENOM --from $ADDRESS_2 \
        --keyring-backend=test --home="$HOME_2" \
        --chain-id="$CHAIN_ID_2" --node "$GAIA_NODE" \
        -y --output json | wait_tx_gaia

  echo "-------------------------- pool delegate -------------------------------------"

  msg=$(printf '{
  "pool_delegate": {
    "pool_addr": "%s",
    "stake_amount": "70000"
  }
  }' "$pool_address")

  tx_result="$(neutrond tx wasm execute "$contract_address" "$msg" \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --amount 2000000untrn \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

  code="$(echo "$tx_result" | jq '.code')"
  if [[ "$code" -ne 0 ]]; then
    echo "Failed to pool delegate: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi

  echo "Waiting 10 seconds for init pool (sometimes it takes a lot of time)…"
  # shellcheck disable=SC2034
  for i in $(seq 10); do
    sleep 1
    echo -n .
  done

  $BINARY query staking delegations "$pool_address" --node "$GAIA_NODE" --output json | jq
}

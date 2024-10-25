#!/usr/bin/env bash

init_pool() {
  echo "-------------------------- init pool -------------------------------------"

msg=$(jq -n \
  --arg ibc_denom "$IBCDENOM" \
  --arg remote_denom "$HOSTCHAINDENOM" \
  --arg fee_receiver "$ADDRESS_1" \
  '{
    init_pool: {
      interchain_account_id: "test1",
      ibc_denom: $ibc_denom,
      channel_id_of_ibc_denom: "channel-0",
      remote_denom: $remote_denom,
      validator_addrs: ["cosmosvaloper18hl5c9xn5dze2g50uaw0l2mr02ew57zk0auktn"],
      platform_fee_receiver: $fee_receiver,
      lsd_token_name: "lsdTokenNameX",
      lsd_token_symbol: "symbolX",
      minimal_stake: "100",
      unbonding_period: 1,
      sdk_greater_or_equal_v047: true
    }
  }')

  # echo "the init_pool msg is: $msg"
  tx_result="$(
    neutrond tx wasm execute "$contract_address" "$msg" \
      --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
      --amount 4200000untrn \
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

}

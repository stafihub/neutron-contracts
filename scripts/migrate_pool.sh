#!/usr/bin/env bash

migrate_pool() {
  echo "-------------------------- migrate pool -------------------------------------"

msg=$(jq -n \
  --arg ibc_denom "$IBCDENOM" \
  --arg remote_denom "$HOSTCHAINDENOM" \
  --arg validator "$VALIDATOR" \
  --arg fee_receiver "$ADDRESS_1" \
  '{
    migrate_pool: {
      interchain_account_id: "test1",
      unbond: "0",
      bond: "0",
      active: "0",
      ibc_denom: $ibc_denom,
      channel_id_of_ibc_denom: "channel-0",
      remote_denom: $remote_denom,
      validator_addrs: [$validator],
      era: 1,
      rate: "1000000",
      total_platform_fee: "0",
      total_lsd_token_amount: "0",
      platform_fee_receiver: $fee_receiver,
      share_tokens: [],
      lsd_token_name: "lsdTokenNameX",
      lsd_token_symbol: "symbolX",
      minimal_stake: "100",
      unbonding_period: 1,
      era_seconds: 20,
      offset: -1000,
      sdk_greater_or_equal_v047: true
    }
  }')

  # echo "the migrate_pool msg is: $msg"
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
    echo "Failed to migrate pool: $(echo "$tx_result" | jq '.raw_log')" && exit 1
  fi

  echo "Waiting 10 seconds for migrate pool (sometimes it takes a lot of time)…"
  # shellcheck disable=SC2034
  for i in $(seq 10); do
    sleep 1
    echo -n .
  done
  echo " done"

  echo "------------------------ pool_info after migrate  ------------------------"
  query="$(printf '{"pool_info": {"pool_addr": "%s"}}' "$pool_address")"
  # echo "$query"
  neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq
  lsd_token_contract_address=$(neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq .data.lsd_token | sed 's/\"//g')
  echo "lsd_token_contract_address: $lsd_token_contract_address"

}

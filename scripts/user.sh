#!/usr/bin/env bash
user_stake() {
    echo "--------------------------user stake through ibc-------------------------------------"

    msg=$(
        cat <<EOF
{
    "wasm": {
        "contract": "$contract_address",
        "msg": {
            "stake": {
                "neutron_address": "$ADDRESS_1",
                "pool_addr": "$pool_address"
            }
        }
    }
}
EOF
    )

    tx_result=$(gaiad tx ibc-transfer transfer transfer channel-0 \
        "$contract_address" 405550000uatom \
        --memo "$msg" \
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
    echo "Waiting 15 seconds for stake (sometimes it takes a lot of time)…"
    # shellcheck disable=SC2034
    for i in $(seq 15); do
        sleep 1
        echo -n .
    done
    echo " done"

    query="$(printf '{"balance": {"address": "%s"}}' "$ADDRESS_1")"
    neutrond query wasm contract-state smart "$lsd_token_contract_address" "$query" --output json | jq

}

user_stake_on_neutron() {
    echo "--------------------------user stake on neutron-------------------------------------"
    query="$(printf '{"balance": {"address": "%s"}}' "$ADDRESS_1")"
    neutrond query wasm contract-state smart "$lsd_token_contract_address" "$query" --output json | jq

    stake_msg=$(printf '{
  "stake": {
    "neutron_address": "%s",
    "pool_addr": "%s"
  }
}' "$ADDRESS_1" "$pool_address")

    tx_result=$(
        neutrond tx wasm execute "$contract_address" "$stake_msg" \
            --amount 1000ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2 \
            --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
            --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
            --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx
    )

    #echo "$tx_result" | jq .
    code="$(echo "$tx_result" | jq '.code')"
    tx_hash="$(echo "$tx_result" | jq '.txhash')"
    if [[ "$code" -ne 0 ]]; then
        echo "Failed to send ibc hook to contract: $(echo "$tx_result" | jq '.raw_log')" && exit 1
    fi
    echo "$tx_hash"
    echo "Waiting 15 seconds for stake (sometimes it takes a lot of time)…"
    # shellcheck disable=SC2034
    for i in $(seq 15); do
        sleep 1
        echo -n .
    done
    echo " done"

    query="$(printf '{"balance": {"address": "%s"}}' "$ADDRESS_1")"
    neutrond query wasm contract-state smart "$lsd_token_contract_address" "$query" --output json | jq

}

user_allowance() {
    echo "--------------------------user allowance-------------------------------------"
    echo "lsd_token allowance"
    allow_msg=$(printf '{
  "increase_allowance": {
    "amount": "11111119999950000",
    "spender": "%s"
  }
}' "$contract_address")

    tx_result="$(neutrond tx wasm execute "$lsd_token_contract_address" "$allow_msg" \
        --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
        --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
        --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

    code="$(echo "$tx_result" | jq '.code')"
    if [[ "$code" -ne 0 ]]; then
        echo "Failed to unstake msg: $(echo "$tx_result" | jq '.raw_log')" && exit 1
    fi
}

user_unstake() {
    echo "--------------------------user unstake-------------------------------------"
    unstake_msg=$(printf '{
  "unstake": {
    "amount": "10000",
    "pool_addr": "%s"
  }
}' "$pool_address")

    tx_result="$(neutrond tx wasm execute "$contract_address" "$unstake_msg" \
        --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
        --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
        --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

    code="$(echo "$tx_result" | jq '.code')"
    if [[ "$code" -ne 0 ]]; then
        echo "Failed to unstake msg: $(echo "$tx_result" | jq '.raw_log')" && exit 1
    fi

    query="$(printf '{"balance": {"address": "%s"}}' "$ADDRESS_1")"
    neutrond query wasm contract-state smart "$lsd_token_contract_address" "$query" --output json | jq
    echo "---------------------------------------------------------------"

    query="$(printf '{"pool_info": {"pool_addr": "%s"}}' "$pool_address")"
    echo "pool_info is: "
    echo "$query"
    neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq

    echo "contract_address balance Query"
    neutrond query bank balances "$contract_address" --node "$NEUTRON_NODE" --output json | jq
}

user_withdraw() {
    echo "--------------------------user withdraw-------------------------------------"

    withdraw_msg=$(printf '{
  "withdraw": {
    "pool_addr": "%s",
    "receiver": "%s",
    "unstake_index_list": [1]
  }
}' "$pool_address" "$ADDRESS_2")

    tx_result="$(neutrond tx wasm execute "$contract_address" "$withdraw_msg" \
        --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
        --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
        --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

    code="$(echo "$tx_result" | jq '.code')"
    if [[ "$code" -ne 0 ]]; then
        echo "Failed to withdraw msg: $(echo "$tx_result" | jq '.raw_log')" && exit 1
    fi

    echo "Waiting 10 seconds for withdraw (sometimes it takes a lot of time)…"
    # shellcheck disable=SC2034
    for i in $(seq 10); do
        sleep 1
        echo -n .
    done
    echo " done"

    echo "pool_address balance Query"
    gaiad query bank balances "$pool_address" --node "$GAIA_NODE" --output json | jq
}

user_stake_lsm() {
    echo "--------------------------user stake lsm-------------------------------------"
    query="$(printf '{"balance": {"address": "%s"}}' "$ADDRESS_1")"
    neutrond query wasm contract-state smart "$lsd_token_contract_address" "$query" --output json | jq

    tx_result=$(gaiad tx staking delegate "$VALIDATOR" 10000uatom \
        --gas auto --gas-adjustment 1.4 \
        --fees 10000uatom --from $ADDRESS_2 \
        --keyring-backend=test --home="$HOME_2" \
        --chain-id="$CHAIN_ID_2" --node "$GAIA_NODE" \
        -y --output json | wait_tx_gaia)

    code="$(echo "$tx_result" | jq '.code')"
    tx_hash="$(echo "$tx_result" | jq '.txhash')"
    if [[ "$code" -ne 0 ]]; then
        echo "Failed to send ibc hook to contract: $(echo "$tx_result" | jq '.raw_log')" && exit 1
    fi
    echo "$tx_hash"

    echo "Waiting 5 seconds for delegate  (sometimes it takes a lot of time)…"
    # shellcheck disable=SC2034
    for i in $(seq 5); do
        sleep 1
        echo -n .
    done
    echo " done"

    tx_result=$(gaiad tx staking tokenize-share "$VALIDATOR" 6000uatom "$ADDRESS_2" \
        --gas auto --gas-adjustment 1.4 \
        --fees 10000uatom --from $ADDRESS_2 \
        --keyring-backend=test --home="$HOME_2" \
        --chain-id="$CHAIN_ID_2" --node "$GAIA_NODE" \
        -y --output json | wait_tx_gaia)
    code="$(echo "$tx_result" | jq '.code')"
    tx_hash="$(echo "$tx_result" | jq '.txhash')"
    if [[ "$code" -ne 0 ]]; then
        echo "Failed to send ibc hook to contract: $(echo "$tx_result" | jq '.raw_log')" && exit 1
    fi
    echo "$tx_hash"

    echo "Waiting 5 seconds for tokenize  (sometimes it takes a lot of time)…"
    # shellcheck disable=SC2034
    for i in $(seq 5); do
        sleep 1
        echo -n .
    done
    echo " done"

    share_token_denom=$(gaiad q bank balances $ADDRESS_2 --node "$GAIA_NODE" --output json | jq ".balances[0].denom" | sed 's/\"//g')
    share_token_amount=$(gaiad q bank balances $ADDRESS_2 --node "$GAIA_NODE" --output json | jq ".balances[0].amount" | sed 's/\"//g')

    msg=$(
        cat <<EOF
{
    "wasm": {
        "contract": "$contract_address",
        "msg": {
            "stake_lsm": {
                "neutron_address": "$ADDRESS_1",
                "pool_addr": "$pool_address"
            }
        }
    }
}
EOF
    )

    tx_result=$(gaiad tx ibc-transfer transfer transfer channel-0 \
        "$contract_address" $share_token_amount$share_token_denom \
        --memo "$msg" \
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

    echo "Waiting 10 seconds for lsd_token mint (sometimes it takes a lot of time)…"
    # shellcheck disable=SC2034
    for i in $(seq 10); do
        sleep 1
        echo -n .
    done
    echo " done"

    query="$(printf '{"balance": {"address": "%s"}}' "$ADDRESS_1")"
    neutrond query wasm contract-state smart "$lsd_token_contract_address" "$query" --output json | jq

    query="$(printf '{"pool_info": {"pool_addr": "%s"}}' "$pool_address")"
    echo "------------------------ pool_info after stake lsm ------------------------"
    neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq
}

#!/usr/bin/env bash

# http://redsymbol.net/articles/unofficial-bash-strict-mode/
set -euo pipefail
IFS=$'\n\t'

CONTRACT_PATH="artifacts/stake_manager.wasm"
RTOKEN_CONTRACT_PATH="artifacts/rtoken.wasm"
CHAIN_ID_1="test-1"
CHAIN_ID_2="test-2"
#NEUTRON_DIR="${NEUTRON_DIR:-/var/lib/docker/volumes/neutron-testing-data/_data}"
#HOME_1="${NEUTRON_DIR}/test-1/"
NEUTRON_DIR="${NEUTRON_DIR:-/Users/wang/OrbStack/docker/volumes}"
HOME_1="${NEUTRON_DIR}/neutron-testing-data/test-1/"
HOME_2="${NEUTRON_DIR}/neutron-testing-data/test-2/"
NEUTRON_NODE="tcp://127.0.0.1:26657"
GAIA_NODE="tcp://127.0.0.1:16657"
ADDRESS_1="neutron1m9l358xunhhwds0568za49mzhvuxx9ux8xafx2"
ADDRESS_2="cosmos10h9stc5v6ntgeygf5xf945njqq5h32r53uquvw"
ADMIN="neutron1m9l358xunhhwds0568za49mzhvuxx9ux8xafx2"
# VALIDATOR="cosmosvaloper18hl5c9xn5dze2g50uaw0l2mr02ew57zk0auktn"
# rtoken_address="neutron1kt4604x3kn48dulhvjzyxekn9xg3xnv8a5f48syr0vtrf3j3nyss50n028"

wait_tx() {
    local txhash
    local attempts
    txhash="$(jq -r '.txhash' </dev/stdin)"
    ((attempts = 50))
    while ! neutrond query tx --type=hash "$txhash" --output json --node "$NEUTRON_NODE" 2>/dev/null; do
        ((attempts -= 1)) || {
            echo "tx $txhash still not included in block" 1>&2
            exit 1
        }
        sleep 0.1
    done
}

wait_tx_gaia() {
    local txhash
    local attempts
    txhash="$(jq -r '.txhash' </dev/stdin)"
    ((attempts = 50))
    while ! gaiad query tx --type=hash "$txhash" --output json --node "$GAIA_NODE" 2>/dev/null; do
        ((attempts -= 1)) || {
            echo "tx $txhash still not included in block" 1>&2
            exit 1
        }
        sleep 0.1
    done
}

code_id="$(neutrond tx wasm store "$CONTRACT_PATH" \
    --from "$ADDRESS_1" --gas 50000000 --chain-id "$CHAIN_ID_1" \
    --broadcast-mode=sync --gas-prices 0.0025untrn -y \
    --output json --keyring-backend=test --home "$HOME_1" \
    --node "$NEUTRON_NODE" |
    wait_tx | jq -r '.logs[0].events[] | select(.type == "store_code").attributes[] | select(.key == "code_id").value')"
echo "Code ID: $code_id"

contract_address=$(neutrond tx wasm instantiate "$code_id" '{}' \
    --from "$ADDRESS_1" --admin "$ADMIN" -y --chain-id "$CHAIN_ID_1" \
    --output json --broadcast-mode=sync --label "init" \
    --keyring-backend=test --gas-prices 0.0025untrn --gas auto \
    --gas-adjustment 1.4 --home "$HOME_1" \
    --node "$NEUTRON_NODE" 2>/dev/null |
    wait_tx | jq -r '.logs[0].events[] | select(.type == "instantiate").attributes[] | select(.key == "_contract_address").value')
echo "Contract address: $contract_address"

tx_result="$(neutrond tx bank send demowallet1 "$contract_address" 1000000untrn \
    --chain-id "$CHAIN_ID_1" --home "$HOME_1" --node "$NEUTRON_NODE" \
    --keyring-backend=test -y --gas-prices 0.0025untrn \
    --broadcast-mode=sync --output json | wait_tx)"

code="$(echo "$tx_result" | jq '.code')"
if [[ "$code" -ne 0 ]]; then
    echo "Failed to send money to contract: $(echo "$tx_result" | jq '.raw_log')" && exit 1
fi
echo "Sent money to contract to pay fees"

msg='{"register_pool":{
  "connection_id": "connection-0",
  "interchain_account_id": "test1",
  "register_fee":[
    {
        "denom":"untrn",
        "amount": "300000"
    }
  ]
}}'

tx_result="$(neutrond tx wasm execute "$contract_address" "$msg" \
    --amount 300000untrn \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0055untrn --gas 2000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

code="$(echo "$tx_result" | jq '.code')"
if [[ "$code" -ne 0 ]]; then
    echo "Failed to register interchain account: $(echo "$tx_result" | jq '.raw_log')" && exit 1
fi

echo "Waiting 10 seconds for interchain account (sometimes it takes a lot of time)…"
# shellcheck disable=SC2034
for i in $(seq 10); do
    sleep 1
    echo -n .
done
echo " done"

query='{"interchain_account_address_from_contract":{"interchain_account_id":"test1"}}'
query_b64_urlenc="$(echo -n "$query" | base64 | tr -d '\n' | jq -sRr '@uri')"
url="http://127.0.0.1:1317/wasm/contract/$contract_address/smart/$query_b64_urlenc?encoding=base64"
echo "interchain_account_address_from_contract query url is: $url"
ica_address=$(curl -s "$url" | jq -r '.result.smart' | base64 -d | jq -r '.[0]')
echo "ICA address: $ica_address"

code_id="$(neutrond tx wasm store "$RTOKEN_CONTRACT_PATH" \
    --from "$ADDRESS_1" --gas 50000000 --chain-id "$CHAIN_ID_1" \
    --broadcast-mode=sync --gas-prices 0.0025untrn -y \
    --output json --keyring-backend=test --home "$HOME_1" \
    --node "$NEUTRON_NODE" |
    wait_tx | jq -r '.logs[0].events[] | select(.type == "store_code").attributes[] | select(.key == "code_id").value')"
echo "Code ID: $code_id"

instantiate_msg=$(printf '{
  "name": "ratom-1",
  "symbol": "ratom",
  "decimals": 6,
  "initial_balances": [],
  "mint": {
    "minter": "%s"
  }
}' "$contract_address")

rtoken_contract_address=$(neutrond tx wasm instantiate "$code_id" "$instantiate_msg" \
    --from "$ADDRESS_1" --admin "$ADMIN" -y --chain-id "$CHAIN_ID_1" \
    --output json --broadcast-mode=sync --label "init" \
    --keyring-backend=test --gas-prices 0.0025untrn --gas auto \
    --gas-adjustment 1.4 --home "$HOME_1" \
    --node "$NEUTRON_NODE" 2>/dev/null |
    wait_tx | jq -r '.logs[0].events[] | select(.type == "instantiate").attributes[] | select(.key == "_contract_address").value')
echo "Rtoken Contract address: $rtoken_contract_address"

query="{\"pool_info\":{\"pool_addr\":\"$ica_address\"}}"
query_b64_urlenc="$(echo -n "$query" | base64 | tr -d '\n' | jq -sRr '@uri')"
url="http://127.0.0.1:1317/wasm/contract/$contract_address/smart/$query_b64_urlenc?encoding=base64"
pool_info=$(curl -s "$url" | jq -r '.result.smart' | base64 -d | jq)
echo "pool_info is: $pool_info"

msg=$(printf '{
  "init_pool": {
    "interchain_account_id": "test1",
    "unbond": "0",
    "active": "0",
    "bond": "0",
    "ibc_denom": "ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2",
    "remote_denom": "uatom",
    "validator_addrs": ["cosmosvaloper18hl5c9xn5dze2g50uaw0l2mr02ew57zk0auktn"],
    "era": 1,
    "rate": "1000000"
  }
}')

# echo "the msg is: $msg"

tx_result="$(neutrond tx wasm execute "$contract_address" "$msg" \
    --amount 2000000untrn \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

code="$(echo "$tx_result" | jq '.code')"
if [[ "$code" -ne 0 ]]; then
    echo "Failed to init pool: $(echo "$tx_result" | jq '.raw_log')" && exit 1
fi

echo "Waiting 5 seconds for init pool (sometimes it takes a lot of time)…"
# shellcheck disable=SC2034
for i in $(seq 5); do
    sleep 1
    echo -n .
done
echo " done"

msg=$(printf '{
  "config_pool": {
    "pool_addr": "%s",
    "rtoken": "%s",
    "protocol_fee_receiver": "%s",
    "minimal_stake": "1000",
    "unstake_times_limit": 10,
    "next_unstake_index": 1,
    "unbonding_period": 2,
    "unbond_commission":"100000",
    "era_seconds": 60,
    "offset": 26657
  }
}' "$ica_address" "$rtoken_contract_address" "$ADDRESS_1")

# echo "the msg is: $msg"

tx_result="$(neutrond tx wasm execute "$contract_address" "$msg" \
    --amount 2000000untrn \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

code="$(echo "$tx_result" | jq '.code')"
if [[ "$code" -ne 0 ]]; then
    echo "Failed to config pool: $(echo "$tx_result" | jq '.raw_log')" && exit 1
fi

echo "Waiting 5 seconds for config pool (sometimes it takes a lot of time)…"
# shellcheck disable=SC2034
for i in $(seq 5); do
    sleep 1
    echo -n .
done
echo " done"

# query="{\"pool_info\":{\"pool_addr\":\"$ica_address\"}}"
# query_b64_urlenc="$(echo -n "$query" | base64 | tr -d '\n' | jq -sRr '@uri')"
# url="http://127.0.0.1:1317/wasm/contract/$contract_address/smart/$query_b64_urlenc?encoding=base64"
# echo "url is: $url"
# pool_info=$(curl -s "$url" | jq -r '.result.smart' | base64 -d | jq)
# echo "pool_info is: $pool_info"

msg=$(
    cat <<EOF
{
    "wasm": {
        "contract": "$contract_address",
        "msg": {
            "stake": {
                "neutron_address": "$ADDRESS_1",
                "pool_addr": "$ica_address"
            }
        }
    }
}
EOF
)

tx_result=$(gaiad tx ibc-transfer transfer transfer channel-0 \
    "$contract_address" 969999970000uatom \
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

echo "Waiting 5 seconds for rtoken mint (sometimes it takes a lot of time)…"
# shellcheck disable=SC2034
for i in $(seq 5); do
    sleep 1
    echo -n .
done
echo " done"

echo "--------------------------user balance-------------------------------------"
query="$(printf '{"balance": {"address": "%s"}}' "$ADDRESS_1")"
neutrond query wasm contract-state smart "$rtoken_contract_address" "$query" --output json | jq

# rtoken allow
allow_msg=$(printf '{
  "increase_allowance": {
    "amount": "11111119999950000",
    "spender": "%s"
  }
}' "$contract_address")

tx_result="$(neutrond tx wasm execute "$rtoken_contract_address" "$allow_msg" \
    --amount 2000000untrn \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

echo "Waiting 5 seconds for rtoken allow msg (sometimes it takes a lot of time)…"
# shellcheck disable=SC2034
for i in $(seq 5); do
    sleep 1
    echo -n .
done
echo " done"

unstake_msg=$(printf '{
  "unstake": {
    "amount": "9999950000",
    "pool_addr": "%s"
  }
}' "$ica_address")

tx_result="$(neutrond tx wasm execute "$contract_address" "$unstake_msg" \
    --amount 2000000untrn \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

code="$(echo "$tx_result" | jq '.code')"
if [[ "$code" -ne 0 ]]; then
    echo "Failed to unstake msg: $(echo "$tx_result" | jq '.raw_log')" && exit 1
fi

echo "Waiting 5 seconds for unstake (sometimes it takes a lot of time)…"
# shellcheck disable=SC2034
for i in $(seq 5); do
    sleep 1
    echo -n .
done
echo " done"

query="$(printf '{"balance": {"address": "%s"}}' "$ADDRESS_1")"
neutrond query wasm contract-state smart "$rtoken_contract_address" "$query" --output json | jq
echo "---------------------------------------------------------------"

query="{\"pool_info\":{\"pool_addr\":\"$ica_address\"}}"
query_b64_urlenc="$(echo -n "$query" | base64 | tr -d '\n' | jq -sRr '@uri')"
url="http://127.0.0.1:1317/wasm/contract/$contract_address/smart/$query_b64_urlenc?encoding=base64"
pool_info=$(curl -s "$url" | jq -r '.result.smart' | base64 -d | jq)
echo "pool_info is: $pool_info"

withdraw_addr=$(echo $pool_info | jq -r '.withdraw_addr')

echo "withdraw_addr: $withdraw_addr"

echo "DelegatorWithdrawAddress Query"
grpcurl -plaintext -d "{\"delegator_address\":\"$ica_address\"}" localhost:9090 cosmos.distribution.v1beta1.Query/DelegatorWithdrawAddress | jq

echo "contract_address balance Query"
neutrond query bank balances "$contract_address" | jq

era_update_msg=$(printf '{
  "era_update": {
    "channel": "channel-0",
    "pool_addr": "%s"
  }
}' "$ica_address")

tx_result="$(neutrond tx wasm execute "$contract_address" "$era_update_msg" \
    --amount 2000000untrn \
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
gaiad query bank balances "$ica_address" | jq

# echo "---------------------------------------------------------------"
# query="{\"pool_info\":{\"pool_addr\":\"$ica_address\"}}"
# query_b64_urlenc="$(echo -n "$query" | base64 | tr -d '\n' | jq -sRr '@uri')"
# url="http://127.0.0.1:1317/wasm/contract/$contract_address/smart/$query_b64_urlenc?encoding=base64"
# pool_info=$(curl -s "$url" | jq -r '.result.smart' | base64 -d | jq)
# echo "pool_info is: $pool_info"

tx_result=$(gaiad tx bank send "$ADDRESS_2" "$ica_address" 50000uatom \
    --chain-id "$CHAIN_ID_2" --broadcast-mode=sync --gas-prices 0.0025uatom \
    -y --output json --keyring-backend=test --home "$HOME_2" --node "$GAIA_NODE" | wait_tx_gaia)
code="$(echo "$tx_result" | jq '.code')"
if [[ "$code" -ne 0 ]]; then
    echo "Failed to send money to ICA: $(echo "$tx_result" | jq '.raw_log')" && exit 1
fi
echo "Sent money to ICA"

gaiad query bank balances "$ica_address" | jq

bond_msg=$(printf '{
  "era_bond": {
    "pool_addr": "%s"
  }
}' "$ica_address")

tx_result="$(neutrond tx wasm execute "$contract_address" "$bond_msg" \
    --amount 2000000untrn \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

code="$(echo "$tx_result" | jq '.code')"
if [[ "$code" -ne 0 ]]; then
    echo "Failed to era_bond msg: $(echo "$tx_result" | jq '.raw_log')" && exit 1
fi

echo "Waiting 20 seconds for era_bond (sometimes it takes a lot of time)…"
# shellcheck disable=SC2034
for i in $(seq 20); do
    sleep 1
    echo -n .
done
echo " done"

gaiad query staking delegations "$ica_address" | jq

gaiad query bank balances "$ica_address" | jq

query="{\"pool_info\":{\"pool_addr\":\"$ica_address\"}}"
query_b64_urlenc="$(echo -n "$query" | base64 | tr -d '\n' | jq -sRr '@uri')"
url="http://127.0.0.1:1317/wasm/contract/$contract_address/smart/$query_b64_urlenc?encoding=base64"
pool_info=$(curl -s "$url" | jq -r '.result.smart' | base64 -d | jq)
echo "pool_info is: $pool_info"

era_collect_withdraw_msg=$(printf '{
  "era_collect_withdraw": {
    "pool_addr": "%s"
  }
}' "$ica_address")

tx_result="$(neutrond tx wasm execute "$contract_address" "$era_collect_withdraw_msg" \
    --amount 2000000untrn \
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

gaiad query bank balances "$ica_address" | jq

era_active_msg=$(printf '{
  "era_active": {
    "pool_addr": "%s"
  }
}' "$ica_address")

tx_result="$(neutrond tx wasm execute "$contract_address" "$era_active_msg" \
    --amount 2000000untrn \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

code="$(echo "$tx_result" | jq '.code')"
if [[ "$code" -ne 0 ]]; then
    echo "Failed to era_active_msg msg: $(echo "$tx_result" | jq '.raw_log')" && exit 1
fi

echo "Waiting 10 seconds for era_collect_withdraw_msg (sometimes it takes a lot of time)…"
# shellcheck disable=SC2034
for i in $(seq 10); do
    sleep 1
    echo -n .
done
echo " done"

query="{\"pool_info\":{\"pool_addr\":\"$ica_address\"}}"
query_b64_urlenc="$(echo -n "$query" | base64 | tr -d '\n' | jq -sRr '@uri')"
url="http://127.0.0.1:1317/wasm/contract/$contract_address/smart/$query_b64_urlenc?encoding=base64"
pool_info=$(curl -s "$url" | jq -r '.result.smart' | base64 -d | jq)
echo "pool_info is: $pool_info"

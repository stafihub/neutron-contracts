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
ica_address=$(curl -s "$url" | jq -r '.result.smart' | base64 -d | jq -r '.[0]')
echo "ICA address: $ica_address"

tx_result=$(gaiad tx bank send "$ADDRESS_2" "$ica_address" 50000uatom \
    --chain-id "$CHAIN_ID_2" --broadcast-mode=sync --gas-prices 0.0025uatom \
    -y --output json --keyring-backend=test --home "$HOME_2" --node "$GAIA_NODE" | wait_tx_gaia)
code="$(echo "$tx_result" | jq '.code')"
if [[ "$code" -ne 0 ]]; then
    echo "Failed to send money to ICA: $(echo "$tx_result" | jq '.raw_log')" && exit 1
fi
echo "Sent money to ICA"

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

msg=$(printf '{
  "config_pool": {
    "interchain_account_id": "test1",
    "need_withdraw": "0",
    "unbond": "0",
    "active": "0",
    "rtoken": "%s",
    "withdraw_addr": "cosmos10h9stc5v6ntgeygf5xf945njqq5h32r53uquvw",
    "ibc_denom": "ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2",
    "remote_denom": "uatom",
    "validator_addrs": ["cosmosvaloper18hl5c9xn5dze2g50uaw0l2mr02ew57zk0auktn"],
    "era": "1",
    "rate": "1000000",
    "minimal_stake": "1000",
    "unstake_times_limit": "10",
    "next_unstake_index": "1",
    "unbonding_period": "2"
  }
}' "$rtoken_contract_address")

tx_result="$(neutrond tx wasm execute "$contract_address" "$msg" \
    --amount 2000000untrn \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

code="$(echo "$tx_result" | jq '.code')"
if [[ "$code" -ne 0 ]]; then
    echo "Failed to config_pool: $(echo "$tx_result" | jq '.raw_log')" && exit 1
fi

echo "Waiting 20 seconds for config_pool (sometimes it takes a lot of time)…"
# shellcheck disable=SC2034
for i in $(seq 20); do
    sleep 1
    echo -n .
done
echo " done"

query="{\"pool_info\":{\"pool_addr\":\"$ica_address\"}}"
query_b64_urlenc="$(echo -n "$query" | base64 | tr -d '\n' | jq -sRr '@uri')"
url="http://127.0.0.1:1317/wasm/contract/$contract_address/smart/$query_b64_urlenc?encoding=base64"
echo "url is: $url"
pool_info=$(curl -s "$url" | jq -r '.result.smart' | base64 -d | jq)
echo "pool_info is: $pool_info"

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

query="$(printf '{"balance": {"address": "%s"}}' "$ADDRESS_1")"
neutrond query wasm contract-state smart "$rtoken_contract_address" "$query" --output json | jq
echo "---------------------------------------------------------------"

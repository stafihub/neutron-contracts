#!/usr/bin/env bash

# http://redsymbol.net/articles/unofficial-bash-strict-mode/
set -euo pipefail
IFS=$'\n\t'

ARCH=$(uname -m)
CONTRACT_PATH="artifacts/lsd_token.wasm"
CW20_ICS0_CONTRACT_PATH="artifacts/cw20_ics20.wasm"
if [ "$ARCH" = "aarch64" ] || [ "$ARCH" = "arm64" ]; then
    CONTRACT_PATH="artifacts/lsd_token-aarch64.wasm"
    CW20_ICS0_CONTRACT_PATH="artifacts/cw20_ics20-aarch64.wasm"
fi

CHAIN_ID_1="test-1"
NEUTRON_DIR="${NEUTRON_DIR:-/Users/$(whoami)/OrbStack/docker/volumes}"
HOME_1="${NEUTRON_DIR}/neutron-testing-data/test-1/"
ADDRESS_1="neutron1m9l358xunhhwds0568za49mzhvuxx9ux8xafx2"
ADDRESS_2="cosmos10h9stc5v6ntgeygf5xf945njqq5h32r53uquvw"
ADMIN="neutron1m9l358xunhhwds0568za49mzhvuxx9ux8xafx2"
NEUTRON_NODE="tcp://127.0.0.1:26657"

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
echo "----------------------------- store lsd token -------------------------------"
code_id="$(neutrond tx wasm store "$CONTRACT_PATH" \
    --from "$ADDRESS_1" --gas 50000000 --chain-id "$CHAIN_ID_1" \
    --broadcast-mode=sync --gas-prices 0.0025untrn -y \
    --output json --keyring-backend=test --home "$HOME_1" \
    --node "$NEUTRON_NODE" |
    wait_tx | jq -r '.logs[0].events[] | select(.type == "store_code").attributes[] | select(.key == "code_id").value')"
echo "Code ID: $code_id"

echo "----------------------------- init lsd token -------------------------------"
instantiate_msg='{
  "name": "ratom-1",
  "symbol": "ratom",
  "decimals": 6,
  "initial_balances": [],
  "mint": {
    "minter": "neutron1m9l358xunhhwds0568za49mzhvuxx9ux8xafx2"
  }
}'

contract_address=$(neutrond tx wasm instantiate "$code_id" "$instantiate_msg" \
    --from "$ADDRESS_1" --admin "$ADMIN" -y --chain-id "$CHAIN_ID_1" \
    --output json --broadcast-mode=sync --label "init" \
    --keyring-backend=test --gas-prices 0.0025untrn --gas auto \
    --gas-adjustment 1.4 --home "$HOME_1" \
    --node "$NEUTRON_NODE" 2>/dev/null |
    wait_tx | jq -r '.logs[0].events[] | select(.type == "instantiate").attributes[] | select(.key == "_contract_address").value')
echo "Contract address: $contract_address"

echo "----------------------------- mint lsd token -------------------------------"

mint_msg=$(printf '{
  "mint": {
    "recipient": "%s",
    "amount": "1000000"
  }
}' "$ADDRESS_1")

tx_result="$(neutrond tx wasm execute "$contract_address" "$mint_msg" \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0055untrn --gas 2000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

code="$(echo "$tx_result" | jq '.code')"
if [[ "$code" -ne 0 ]]; then
    echo "Failed to register interchain account: $(echo "$tx_result" | jq '.raw_log')" && exit 1
fi

echo "----------------------------- store cw20 ics20 contract -------------------------------"

ics_code_id="$(neutrond tx wasm store "$CW20_ICS0_CONTRACT_PATH" \
    --from "$ADDRESS_1" --gas 50000000 --chain-id "$CHAIN_ID_1" \
    --broadcast-mode=sync --gas-prices 0.0025untrn -y \
    --output json --keyring-backend=test --home "$HOME_1" \
    --node "$NEUTRON_NODE" |
    wait_tx | jq -r '.logs[0].events[] | select(.type == "store_code").attributes[] | select(.key == "code_id").value')"
echo "ics_code_id: $ics_code_id"

init_ics20_msg='{"default_timeout":1000,"gov_contract":"'$ADDRESS_1'","allowlist":[{"contract":"'$contract_address'","gas_limit": 10000000}],"default_gas_limit": 10000000}'

echo "----------------------------- init cw20 ics20 contract -------------------------------"

ics20_contract_address=$(neutrond tx wasm instantiate "$ics_code_id" "$init_ics20_msg" \
    --from "$ADDRESS_1" --admin "$ADMIN" -y --chain-id "$CHAIN_ID_1" \
    --output json --broadcast-mode=sync --label "init-ics20" \
    --keyring-backend=test --gas-prices 0.0025untrn --gas auto \
    --gas-adjustment 1.4 --home "$HOME_1" \
    --node "$NEUTRON_NODE" 2>/dev/null |
    wait_tx | jq -r '.logs[0].events[] | select(.type == "instantiate").attributes[] | select(.key == "_contract_address").value')
echo "CW20 ICS20 Contract address: $ics20_contract_address"

echo "----------------------------- get cw20 ics20 ibc port -------------------------------"

CW20_ICS20_QUERY='{"port":{}}'

CW20_ICS20_PORT=$(neutrond q wasm \
    contract-state smart \
    $ics20_contract_address "$CW20_ICS20_QUERY" \
    -o json | jq -r '.data.port_id')
echo "CW20_ICS20_PORT: $CW20_ICS20_PORT"

echo "----------------------------- create ibc channel for cw20 ics20 -------------------------------"

# docker-compose exec relayer bash

# $CW20_ICS20_PORT=""

# . ./network/hermes/variables.sh

# $HERMES_BINARY --config $CONFIG_DIR create channel --order unordered     --a-chain test-1     --a-connection connection-0     --a-port "$CW20_ICS20_PORT"     --b-port transfer     --channel-version ics20-1

# Code ID: 32
# Contract address: neutron1wpqx4mhe5hmgte8s4etam4syfxjt83zwvejhgsmcludfpt5hd6kquxtjd3
# ics_code_id: 33
# CW20 ICS20 Contract address: neutron19c4u4qkakm8339mrryxgr2tv9wmggf9jfh6h0clt8j8dt73nahgqgc8tlq
# CW20_ICS20_PORT: wasm.neutron19c4u4qkakm8339mrryxgr2tv9wmggf9jfh6h0clt8j8dt73nahgqgc8tlq

channel_msg=$(printf '{
    "channel":"channel-3",
    "remote_address":"%s"
}' "$ADDRESS_2")

channel_msg_base64="$(echo -n "$channel_msg" | base64 | tr -d '\n' | jq -sRr '@uri')"

lsd_token_send_msg=$(printf '{
    "send":
    {
      "contract":"%s",
      "amount":"100000",
      "msg":"%s"
    }
}' "$ics20_contract_address" "$channel_msg_base64")

tx_result="$(neutrond tx wasm execute "$contract_address" "$lsd_token_send_msg" \
    --from "$ADDRESS_1" -y --chain-id "$CHAIN_ID_1" --output json \
    --broadcast-mode=sync --gas-prices 0.0055untrn --gas 2000000 \
    --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

code="$(echo "$tx_result" | jq '.code')"
if [[ "$code" -ne 0 ]]; then
    echo "Failed to register interchain account: $(echo "$tx_result" | jq '.raw_log')" && exit 1
fi

#!/usr/bin/env bash

# http://redsymbol.net/articles/unofficial-bash-strict-mode/
set -euo pipefail
IFS=$'\n\t'

CONTRACT_PATH="artifacts/stake_manager.wasm"
CHAIN_ID_1="test-1"
NEUTRON_DIR="${NEUTRON_DIR:-/Users/wang/OrbStack/docker/volumes}"
HOME_1="${NEUTRON_DIR}/neutron-testing-data/test-1/"
ADDRESS_1="neutron1m9l358xunhhwds0568za49mzhvuxx9ux8xafx2"
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

tx_result="$(neutrond tx bank send demowallet1 "$contract_address" 100000untrn \
    --chain-id "$CHAIN_ID_1" --home "$HOME_1" --node "$NEUTRON_NODE" \
    --keyring-backend=test -y --gas-prices 0.0025untrn \
    --broadcast-mode=sync --output json | wait_tx)"

code="$(echo "$tx_result" | jq '.code')"
if [[ "$code" -ne 0 ]]; then
    echo "Failed to send money to contract: $(echo "$tx_result" | jq '.raw_log')" && exit 1
fi
echo "Sent money to contract to pay fees"

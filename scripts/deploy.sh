#!/usr/bin/env bash

deploy() {

    echo "--------------------------deploy stakemanager-------------------------------------"
    stake_manager_code_id="$(neutrond tx wasm store "$CONTRACT_PATH" \
        --from "$ADDRESS_1" --gas 50000000 --chain-id "$CHAIN_ID_1" \
        --broadcast-mode=sync --gas-prices 0.0025untrn -y \
        --output json --keyring-backend=test --home "$HOME_1" \
        --node "$NEUTRON_NODE" |
        wait_tx | jq -r '.logs[0].events[] | select(.type == "store_code").attributes[] | select(.key == "code_id").value')"
    echo "stake manager Code ID: $stake_manager_code_id"

    echo "--------------------------depoly lsd token -------------------------------------"

    lsd_code_id="$(neutrond tx wasm store "$RTOKEN_CONTRACT_PATH" \
        --from "$ADDRESS_1" --gas 50000000 --chain-id "$CHAIN_ID_1" \
        --broadcast-mode=sync --gas-prices 0.0025untrn -y \
        --output json --keyring-backend=test --home "$HOME_1" \
        --node "$NEUTRON_NODE" |
        wait_tx | jq -r '.logs[0].events[] | select(.type == "store_code").attributes[] | select(.key == "code_id").value')"
    echo "lsd token Code ID: $lsd_code_id"

}

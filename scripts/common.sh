#!/usr/bin/env bash

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
        sleep 0.5
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
        sleep 0.5
    done
}

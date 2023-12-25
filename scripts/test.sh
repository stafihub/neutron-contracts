#!/usr/bin/env bash

# http://redsymbol.net/articles/unofficial-bash-strict-mode/
set -euo pipefail
IFS=$'\n\t'

CHAIN_ID_1="test-1"
NEUTRON_DIR="${NEUTRON_DIR:-/Users/wang/OrbStack/docker/volumes}"
HOME_1="${NEUTRON_DIR}/neutron-testing-data/test-1/"
ADDRESS_1="neutron1m9l358xunhhwds0568za49mzhvuxx9ux8xafx2"
NEUTRON_NODE="tcp://127.0.0.1:26657"
# VALIDATOR="cosmosvaloper18hl5c9xn5dze2g50uaw0l2mr02ew57zk0auktn"

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

contract_address="neutron14hpg9wup26t2t2ppfjfaj6w3knxrwasw9sjwzhd7ru0gl83xkxxqv2phqg"

msg='{
  "config_pool": {
    "interchain_account_id": "test1",
    "need_withdraw": "0",
    "unbond": "0",
    "active": "0",
    "rtoken": "neutron1t2tcfr92kgh6j3vg7e70csrgwpwqtdcg2m0umvya6922xype5trqdn6ahj",
    "withdraw_addr": "cosmos10h9stc5v6ntgeygf5xf945njqq5h32r53uquvw",
    "ibc_denom": "ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2",
    "remote_denom": "uatom",
    "validator_addrs": ["cosmosvaloper18hl5c9xn5dze2g50uaw0l2mr02ew57zk0auktn"],
    "era": "1",
    "rate": "1",
    "minimal_stake": "1000",
    "unstake_times_limit": "10",
    "next_unstake_index": "1",
    "unbonding_period": "2"
  }
}'

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

ica_address="cosmos14820s7w9v7hag7k7gkl82ayse0wk4e9xp5ww7xr3ng045myty63qdnss02"

query="{\"pool_info\":{\"pool_addr\":\"$ica_address\"}}"
query_b64_urlenc="$(echo -n "$query" | base64 | tr -d '\n' | jq -sRr '@uri')"
url="http://127.0.0.1:1317/wasm/contract/$contract_address/smart/$query_b64_urlenc?encoding=base64"
echo "url is: $url"
pool_info=$(curl -s "$url" | jq -r '.result.smart' | base64 -d | jq)
echo "pool_info is: $pool_info"

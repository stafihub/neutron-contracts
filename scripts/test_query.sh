#!/usr/bin/env bash

# http://redsymbol.net/articles/unofficial-bash-strict-mode/
set -euo pipefail
IFS=$'\n\t'

NEUTRON_NODE="tcp://127.0.0.1:26657"

contract_address="neutron1nxshmmwrvxa2cp80nwvf03t8u5kvl2ttr8m8f43vamudsqrdvs8qqvfwpj"
ica_address="cosmos1t0f5uk8ukxdy26q5kt73eap2tauvha45usug3s9k5s8d4lkatrwqs2h624"

query="{\"pool_info\":{\"pool_addr\":\"$ica_address\"}}"
echo "query is: $query"
query_b64_urlenc="$(echo -n "$query" | base64 | tr -d '\n' | jq -sRr '@uri')"
url="http://127.0.0.1:1317/wasm/contract/$contract_address/smart/$query_b64_urlenc?encoding=base64"
echo "url is: $url"
pool_info=$(curl -s "$url" | jq -r '.result.smart' | base64 -d | jq)
echo "pool_info is: $pool_info"

echo "---------------------------------------------------------------"

query="{\"interchain_account_address_from_contract\":{\"interchain_account_id\":\"test1\"}}"
echo "query is: $query"
query_b64_urlenc="$(echo -n "$query" | base64 | tr -d '\n' | jq -sRr '@uri')"
url="http://127.0.0.1:1317/wasm/contract/$contract_address/smart/$query_b64_urlenc?encoding=base64"
echo "url is: $url"
query_result=$(curl -s "$url" | jq -r '.result.smart' | base64 -d | jq)
echo "query_result is: $query_result"

echo "---------------------------------------------------------------"

query_id=3
query="$(printf '{"get_registered_query": {"query_id": %s}}' "$query_id")"
neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq

echo "---------------------------------------------------------------"

query_id=2
query="$(printf '{"balance": {"query_id": %s}}' "$query_id")"
neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq
echo "---------------------------------------------------------------"


contract_address="neutron1vlqvaptpe7hztjj52whxk5mm6k2ey9hu4rv5ueu89shagk72mewq6jnxez"
query='{"stack_info":{}}'
neutrond query wasm contract-state smart "$contract_address" "$query" --output json | jq
echo "---------------------------------------------------------------"

ica_addr="cosmos1rdjunm3uslylh7z5kegy4zjc6cjcpn952ah6pnwd0mrd5u6r5x4scdyman"
query="$(printf '{"balance": {"ica_addr": "%s"}}' "$ica_addr")"
neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq
echo "---------------------------------------------------------------"
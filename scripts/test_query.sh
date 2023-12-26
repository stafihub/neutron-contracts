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
# withdraw_addr="cosmos10h9stc5v6ntgeygf5xf945njqq5h32r53uquvw"query_id=3
echo "---------------------------------------------------------------"


rtoken_contract_address="neutron1v6a0pv0pd3etcd7atyr06efg4q56p4czu30lfycff7sf0n55ed8qhvfcl2"
ADDRESS_1="neutron1m9l358xunhhwds0568za49mzhvuxx9ux8xafx2"
query="$(printf '{"balance": {"address": "%s"}}' "$ADDRESS_1")"
neutrond query wasm contract-state smart "$rtoken_contract_address" "$query" --output json | jq
# withdraw_addr="cosmos10h9stc5v6ntgeygf5xf945njqq5h32r53uquvw"query_id=3
echo "---------------------------------------------------------------"

# query_id=4
# contract_address="neutron153vp64h0jlenzwspje0qza5lz8px9sdf63hcdaljtqcghewgl32sh9pe2f"
# query="$(printf '{"bank_total_supply": {"query_id": %s}}' "$query_id")"
# # query="$(printf '{"balance": {"query_id": %s}}' "$query_id")"
# neutrond query wasm contract-state smart "$contract_address" "$query" --node "$NEUTRON_NODE" --output json | jq
# withdraw_addr="cosmos10h9stc5v6ntgeygf5xf945njqq5h32r53uquvw"

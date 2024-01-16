# echo "testnet add validator should success, localdev should Failed"
# pool_add_validator_msg=$(printf '{
#   "pool_add_validator": {
#     "pool_addr": "%s",
#     "validator_addrs": ["cosmosvaloper18ruzecmqj9pv8ac0gvkgryuc7u004te9rh7w5s"]
#   }
# }' "$pool_address")

# tx_result="$(neutrond tx wasm execute "$contract_address" "$pool_add_validator_msg" \
#     --from "demowallet1" -y --chain-id "$CHAIN_ID_1" --output json \
#     --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
#     --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

# code="$(echo "$tx_result" | jq '.code')"
# if [[ "$code" -ne 0 ]]; then
#     echo "Failed to pool_add_validator msg: $(echo "$tx_result" | jq '.raw_log')" && exit 1
# fi

# echo "Waiting 10 seconds for pool_add_validator (sometimes it takes a lot of time)…"
# # shellcheck disable=SC2034
# for i in $(seq 5); do
#     sleep 1
#     echo -n .
# done
# echo " done"

# query="{\"pool_info\":{\"pool_addr\":\"$pool_address\"}}"
# query_b64_urlenc="$(echo -n "$query" | base64 | tr -d '\n' | jq -sRr '@uri')"
# url="http://127.0.0.1:1317/wasm/contract/$contract_address/smart/$query_b64_urlenc?encoding=base64"
# pool_info=$(curl -s "$url" | jq -r '.result.smart' | base64 -d | jq)
# echo "pool_info is: $pool_info"

# echo "rm validator should success"
# pool_rm_validator_msg=$(printf '{
#   "pool_rm_validator": {
#     "pool_addr": "%s",
#     "validator_addr": "cosmosvaloper18ruzecmqj9pv8ac0gvkgryuc7u004te9rh7w5s"
#   }
# }' "$pool_address")

# tx_result="$(neutrond tx wasm execute "$contract_address" "$pool_rm_validator_msg" \
#     --from "demowallet1" -y --chain-id "$CHAIN_ID_1" --output json \
#     --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
#     --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

# code="$(echo "$tx_result" | jq '.code')"
# if [[ "$code" -ne 0 ]]; then
#     echo "Failed to pool_rm_validator_msg msg: $(echo "$tx_result" | jq '.raw_log')" && exit 1
# fi

# echo "Waiting 10 seconds for pool_rm_validator_msg (sometimes it takes a lot of time)…"
# # shellcheck disable=SC2034
# for i in $(seq 5); do
#     sleep 1
#     echo -n .
# done
# echo " done"

# query="{\"pool_info\":{\"pool_addr\":\"$pool_address\"}}"
# query_b64_urlenc="$(echo -n "$query" | base64 | tr -d '\n' | jq -sRr '@uri')"
# url="http://127.0.0.1:1317/wasm/contract/$contract_address/smart/$query_b64_urlenc?encoding=base64"
# pool_info=$(curl -s "$url" | jq -r '.result.smart' | base64 -d | jq)
# echo "pool_info is: $pool_info"

# echo "add validator should Failed"
# pool_add_validator_msg=$(printf '{
#   "pool_add_validator": {
#     "pool_addr": "%s",
#     "validator_addrs": ["cosmosvaloper18ruzecmqj9pv8ac0gvkgryuc7u004te9rh7w5s"]
#   }
# }' "$pool_address")

# tx_result="$(neutrond tx wasm execute "$contract_address" "$pool_add_validator_msg" \
#     --from "demowallet2" -y --chain-id "$CHAIN_ID_1" --output json \
#     --broadcast-mode=sync --gas-prices 0.0025untrn --gas 1000000 \
#     --keyring-backend=test --home "$HOME_1" --node "$NEUTRON_NODE" | wait_tx)"

# code="$(echo "$tx_result" | jq '.code')"
# if [[ "$code" -ne 0 ]]; then
#     echo "Failed to pool_add_validator msg: $(echo "$tx_result" | jq '.raw_log')" && exit 1
# fi

# echo "Waiting 10 seconds for pool_add_validator (sometimes it takes a lot of time)…"
# # shellcheck disable=SC2034
# for i in $(seq 5); do
#     sleep 1
#     echo -n .
# done
# echo " done"

# query="{\"pool_info\":{\"pool_addr\":\"$pool_address\"}}"
# query_b64_urlenc="$(echo -n "$query" | base64 | tr -d '\n' | jq -sRr '@uri')"
# url="http://127.0.0.1:1317/wasm/contract/$contract_address/smart/$query_b64_urlenc?encoding=base64"
# pool_info=$(curl -s "$url" | jq -r '.result.smart' | base64 -d | jq)
# echo "pool_info is: $pool_info"

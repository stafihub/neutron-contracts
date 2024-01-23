#!/usr/bin/env bash

# http://redsymbol.net/articles/unofficial-bash-strict-mode/
. ./scripts/common.sh
. ./scripts/deploy.sh
. ./scripts/init_stack.sh
. ./scripts/config_pool.sh
. ./scripts/init_bridge.sh
. ./scripts/era.sh
. ./scripts/user.sh

# create stake-manager contract -> create lsd_token contract --> send gas to stake manager -> test stake -> test unstake -> test new era

set -euo pipefail
IFS=$'\n\t'
ARCH=$(uname -m)
CONTRACT_PATH="artifacts/stake_manager.wasm"
RTOKEN_CONTRACT_PATH="artifacts/lsd_token.wasm"
BRIDGE_CONTRACT_PATH="artifacts/bridge.wasm"
if [ "$ARCH" = "aarch64" ] || [ "$ARCH" = "arm64" ]; then
    CONTRACT_PATH="artifacts/stake_manager-aarch64.wasm"
    RTOKEN_CONTRACT_PATH="artifacts/lsd_token-aarch64.wasm"
    BRIDGE_CONTRACT_PATH="artifacts/bridge-aarch64.wasm"
fi

CHAIN_ID_1="test-1"
CHAIN_ID_2="test-2"
#NEUTRON_DIR="${NEUTRON_DIR:-/var/lib/docker/volumes/neutron-testing-data/_data}"
#HOME_1="${NEUTRON_DIR}/test-1/"
NEUTRON_DIR="${NEUTRON_DIR:-/Users/$(whoami)/OrbStack/docker/volumes}"
echo "volumes path: $NEUTRON_DIR"
HOME_1="${NEUTRON_DIR}/neutron-testing-data/test-1/"
HOME_2="${NEUTRON_DIR}/neutron-testing-data/test-2/"
NEUTRON_NODE="tcp://127.0.0.1:26657"
GAIA_NODE="tcp://127.0.0.1:16657"
ADDRESS_1="neutron1m9l358xunhhwds0568za49mzhvuxx9ux8xafx2"
ADDRESS_2="cosmos10h9stc5v6ntgeygf5xf945njqq5h32r53uquvw"
ADMIN="neutron1m9l358xunhhwds0568za49mzhvuxx9ux8xafx2"
VALIDATOR="cosmosvaloper18hl5c9xn5dze2g50uaw0l2mr02ew57zk0auktn"

deploy
init_stack
config_pool
init_bridge

user_stake_lsm
user_stake
user_allowance
user_unstake

redeem_token_for_share

process_era
user_unstake
process_era

user_withdraw

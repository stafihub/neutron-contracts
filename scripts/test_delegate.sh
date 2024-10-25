#!/usr/bin/env bash

# http://redsymbol.net/articles/unofficial-bash-strict-mode/
. ./scripts/common.sh
. ./scripts/deploy.sh
. ./scripts/init_stack.sh
. ./scripts/register_pool.sh
. ./scripts/init_pool.sh
. ./scripts/migrate_pool.sh
. ./scripts/pool_delegate.sh

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

BINARY=gaiad
HOSTCHAINDENOM="uatom"
IBCDENOM="ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2"


NEUTRON_DIR="${NEUTRON_DIR:-/Users/$(whoami)/OrbStack/docker/volumes}"
HOME_1="${HOME_1:-${NEUTRON_DIR}/neutron-testing-data/${CHAIN_ID_1}/}"
HOME_2="${HOME_2:-${NEUTRON_DIR}/neutron-testing-data/${CHAIN_ID_2}/}"

# NEUTRON_DIR="${NEUTRON_DIR:-/var/lib/docker/volumes/neutron-testing-data/_data}"
# HOME_1="${NEUTRON_DIR}/${CHAIN_ID_1}/"
# HOME_2="${NEUTRON_DIR}/${CHAIN_ID_2}/"

echo "volumes path: $NEUTRON_DIR"
echo "home 1 path: $HOME_1"
echo "home 2 path: $HOME_2"
NEUTRON_NODE="tcp://127.0.0.1:26657"
GAIA_NODE="tcp://127.0.0.1:16657"
ADDRESS_1="neutron1m9l358xunhhwds0568za49mzhvuxx9ux8xafx2"
ADDRESS_2="cosmos10h9stc5v6ntgeygf5xf945njqq5h32r53uquvw"
ADMIN="neutron1m9l358xunhhwds0568za49mzhvuxx9ux8xafx2"
VALIDATOR="cosmosvaloper18hl5c9xn5dze2g50uaw0l2mr02ew57zk0auktn"

deploy
init_stack
register_pool
migrate_pool
pool_delegate

#!/usr/bin/env bash
# Nexora deploy script — build and deploy Soroban contracts
#
# Usage:   ./deploy.sh [--wasm]   (wasm-only)  |  ./deploy.sh  (full deploy)
#          NETWORK=testnet ./deploy.sh          (override network)
#
# Prerequisites: soroban-cli, cargo, rustup target add wasm32-unknown-unknown
# deploy.sh — Deploy Nexora core contract and store the contract ID.
#
# Usage: bash deploy.sh
#
# Environment:
#   SOROBAN_NETWORK   — testnet (default) | mainnet
#   SOROBAN_ACCOUNT   — source account name
#
# Closes #96

set -euo pipefail

NETWORK="${SOROBAN_NETWORK:-testnet}"
WASM_PATH="target/wasm32-unknown-unknown/release/nexora_core.wasm"
CONTRACT_ID_FILE=".nexora_contract_id"

echo "🔨 Building contract for network: $NETWORK"
cargo build -p nexora-core --target wasm32-unknown-unknown --release

if [ ! -f "$WASM_PATH" ]; then
  echo "❌ WASM file not found at $WASM_PATH"
  exit 1
fi

echo "🚀 Deploying to network: $NETWORK"
CONTRACT_ID=$(stellar contract deploy \
  --wasm "$WASM_PATH" \
  --network "$NETWORK" \
  --source "${SOROBAN_ACCOUNT:-default}")

echo "✅ Contract deployed successfully!"
echo "📝 Contract ID: $CONTRACT_ID"

echo "$CONTRACT_ID" > "$CONTRACT_ID_FILE"
echo "✅ Contract ID stored in $CONTRACT_ID_FILE"

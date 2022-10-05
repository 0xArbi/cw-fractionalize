#!/bin/bash

# takes several arguments:
# 1: address to use for instantiating
# 2: key to use as --from argument
BINARY='junod'
DENOM='ujuno'
CHAIN_ID='juno-1'
RPC='https://rpc-juno-ia.cosmosia.notional.ventures:443'
LABEL="cw-fractionalize"
TXFLAG="--gas-prices 0.01$DENOM --gas auto --gas-adjustment 2 -y -b block --chain-id $CHAIN_ID --node $RPC"

# compile
# docker run --rm -v "$(pwd)":/code \
#   --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
#   --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
#   cosmwasm/rust-optimizer:0.12.6
RUSTFLAGS='-C link-arg=-s' cargo wasm

# presumably you know the addr you want to use already
echo "Address to deploy contracts: $1"
echo "TX Flags: $TXFLAG"

# upload wasm
CONTRACT_CODE=$($BINARY tx wasm store ./target/wasm32-unknown-unknown/release/cw_fractionalize.wasm --from $2 $TXFLAG --output json | jq -r '.logs[0].events[-1].attributes[0].value')

echo "Stored: $CONTRACT_CODE"

# instantiate the CW721
INIT='{}'
echo "$INIT" | jq .
$BINARY tx wasm instantiate $CONTRACT_CODE "$INIT" --from "$2" --label $LABEL $TXFLAG --no-admin

# get contract addr
CONTRACT_ADDRESS=$($BINARY q wasm list-contract-by-code $CONTRACT_CODE --output json | jq -r '.contracts[-1]')

# Print out config variables
printf "\n ------------------------ \n"
printf "Config Variables \n\n"

echo "FRACTIONALIZER_CODE_ID=$CONTRACT_CODE"
echo "FRACTIONALIZER_ADDRESS=$CONTRACT_ADDRESS"
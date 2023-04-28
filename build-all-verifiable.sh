#!/usr/bin/env bash -eu

contracts=( "azns_name_checker" "azns_fee_calculator" "azns_merkle_verifier" "azns_registry" "azns_router" )

DIR="${DIR:=./deployments-verifiable}"

for i in "${contracts[@]}"
do  
  echo -e "\nBuilding './$i/Cargo.toml'…"
  # Install via: cargo install --git https://github.com/web3labs/ink-verifier-image.git
  build-verifiable-ink -i ghcr.io/web3labs/ink-verifier $i/Cargo.toml

  echo "Copying build files to '$DIR/$i/'…"
  mkdir -p $DIR/$i
  cp ./target/ink/$i/$i.contract $DIR/$i/
  cp ./target/ink/$i/$i.wasm $DIR/$i/
  cp ./target/ink/$i/$i.json $DIR/$i/metadata.json
done
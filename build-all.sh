#!/usr/bin/env bash -eu

contracts=( "azns_name_checker" "azns_fee_calculator" "azns_merkle_verifier" "azns_registry" )

for i in "${contracts[@]}"
do
  echo -e "\nBuilding './$i/Cargo.toml'…"
  cargo contract build --release --quiet --manifest-path $i/Cargo.toml

  echo "Copying build files to './deployments/$i/'…"
  mkdir -p ./deployments/$i
  cp ./target/ink/$i/$i.contract ./deployments/$i/
  cp ./target/ink/$i/$i.wasm ./deployments/$i/
  cp ./target/ink/$i/$i.json ./deployments/$i/metadata.json
done
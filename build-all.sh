#!/usr/bin/env bash -eu

contracts=( "azns_name_checker" "azns_fee_calculator" "azns_merkle_verifier" "azns_registry" )

DIR="${DIR:=./deployments}"

for i in "${contracts[@]}"
do
  echo -e "\nBuilding './$i/Cargo.toml'…"
  cargo contract build --release --quiet --manifest-path $i/Cargo.toml

  echo "Copying build files to '$DIR/$i/'…"
  mkdir -p $DIR/$i
  cp ./target/ink/$i/$i.contract $DIR/$i/
  cp ./target/ink/$i/$i.wasm $DIR/$i/
  cp ./target/ink/$i/$i.json $DIR/$i/metadata.json
done
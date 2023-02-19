#!/usr/bin/env bash -eu

contracts=( "azns_name_checker" "azns_fee_calculator" "merkle_verifier" "azns_registry" )

for i in "${contracts[@]}"
do
  echo -e "\Testing './$i/Cargo.toml'…"
  cargo test --manifest-path $i/Cargo.toml
done
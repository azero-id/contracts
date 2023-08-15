#!/usr/bin/env bash -eu

contracts=( "azns_name_checker" "azns_fee_calculator" "azns_merkle_verifier" "azns_registry" "azns_router" )

DIR="${DIR:=./deployments}"

for i in "${contracts[@]}"
do
  echo -e "\nMinifying '$DIR/$i/metadata.json'…"
  # cat $DIR/$i/metadata.json | jq -c '.' > $DIR/$i/metadata.json
  cat $DIR/$i/metadata.json > $DIR/$i/metadata.tmp.json && jq -c '.' $DIR/$i/metadata.tmp.json > $DIR/$i/metadata.json && rm $DIR/$i/metadata.tmp.json
done
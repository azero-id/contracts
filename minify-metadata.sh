#!/usr/bin/env bash
set -eu

# ENVIRONMENT VARIABLES
DIR="${DIR:=./deployments}" # Output directory for build files

# Store all folder names under `CONTRACTS_DIR` in an array
contracts=()
for d in $DIR/* ; do
  if [ -d "$d" ] && [ -f "$d/metadata.json" ]; then
    contracts+=($(basename $d))
  fi
done

# Build all contracts
for i in "${contracts[@]}"
do
  echo -e "\nMinifying '$DIR/$i/metadata.json'…"
  # cat $DIR/$i/metadata.json | jq -c '.' > $DIR/$i/metadata.json
  cat $DIR/$i/metadata.json > $DIR/$i/metadata.tmp.json && jq -c '.' $DIR/$i/metadata.tmp.json > $DIR/$i/metadata.json && rm $DIR/$i/metadata.tmp.json
done
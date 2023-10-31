#!/usr/bin/env bash
set -eu

# ENVIRONMENT VARIABLES
DIR="${DIR:=./deployments}" # Output directory for build files
CONTRACTS_DIR="${CONTRACTS_DIR:=./src}" # Base contract directory 

# Copy command helper (cross-platform)
CP_CMD=$(command -v cp &> /dev/null && echo "cp" || echo "copy")

# Store all folder names under `CONTRACTS_DIR` in an array
contracts=()
for d in $CONTRACTS_DIR/* ; do
  if [ -d "$d" ] && [ -f "$d/Cargo.toml" ]; then
    contracts+=($(basename $d))
  fi
done

# Build all contracts
for i in "${contracts[@]}"
do
  echo -e "\nBuilding '$CONTRACTS_DIR/$i/Cargo.toml'…"
  cargo contract build --release --quiet --manifest-path $CONTRACTS_DIR/$i/Cargo.toml

  echo "Copying build files to '$DIR/$i/'…"
  mkdir -p $DIR/$i
  $CP_CMD ./target/ink/$i/$i.contract $DIR/$i/
  $CP_CMD ./target/ink/$i/$i.wasm $DIR/$i/
  $CP_CMD ./target/ink/$i/$i.json $DIR/$i/metadata.json
done
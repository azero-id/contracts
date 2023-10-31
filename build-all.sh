#!/usr/bin/env bash
set -eu

# ENVIRONMENT VARIABLES
DIR="${DIR:=./deployments}" # Output directory for build files
CONTRACTS_DIR="${CONTRACTS_DIR:=./src}" # Base contract directory 

# Copy command helper (cross-platform)
CP_CMD=$(command -v cp &> /dev/null && echo "cp" || echo "copy")

# Determine all contracts under `$CONTRACTS_DIR`
contracts=($(find $CONTRACTS_DIR -maxdepth 1 -type d -exec test -f {}/Cargo.toml \; -print | xargs -n 1 basename))

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
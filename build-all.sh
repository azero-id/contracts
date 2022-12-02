#!/usr/bin/env bash -eu

cargo +stable contract build --optimization-passes=0 --release --manifest-path azns_name_checker/Cargo.toml
cargo +stable contract build --optimization-passes=0 --release --manifest-path azns_registry/Cargo.toml

mkdir -p target
cp azns_name_checker/target/ink/azns_name_checker.contract target/
cp azns_registry/target/ink/azns_registry.contract target/
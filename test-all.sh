#!/usr/bin/env bash -eu

cargo +stable contract test --manifest-path azns_name_checker/Cargo.toml
cargo +stable contract test --manifest-path azns_registry/Cargo.toml

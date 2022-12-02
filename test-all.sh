#!/usr/bin/env bash -eu

cargo +nightly contract test --manifest-path azns_registry/Cargo.toml

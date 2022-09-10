#!/usr/bin/env bash -eu

cargo +nightly contract test --manifest-path azd_registry/Cargo.toml

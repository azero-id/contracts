

#!/usr/bin/env bash

set -eu

cargo +stable contract build --optimization-passes=0 --release --manifest-path azns_registry/name_checker/Cargo.toml
cargo +stable contract build --optimization-passes=0 --release --manifest-path azns_registry/Cargo.toml

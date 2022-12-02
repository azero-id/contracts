#!/usr/bin/env bash -eu

# cargo contract upload --manifest-path azns_name_checker/Cargo.toml \
#   --suri //Alice 

# cargo contract upload --manifest-path azns_registry/Cargo.toml \
#   --suri //Alice 

cargo contract instantiate --manifest-path azns_name_checker/Cargo.toml \
  --suri //alice \
  --constructor new \
  --skip-confirm

cargo contract instantiate --manifest-path azns_registry/Cargo.toml \
  --suri //alice \
  --constructor new \
  --args 0xdf47f99616c2b831659671656a06c296ff2e1d9ab858bf0009ff2c6bba0416a1 0 \
  --skip-confirm

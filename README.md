# AZERO.ID – Smart Contracts

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Built with ink!](https://raw.githubusercontent.com/paritytech/ink/master/.images/badge.svg)](https://github.com/paritytech/ink)

This repository contains the smart contracts of [AZERO.ID](https://azero.id).

Find our full documentation here: **https://docs.azero.id/**.

## Development

```bash
# Prerequisites (Rust, Cargo, ink! CLI, Substrate Contracts Node)
# See: https://github.com/scio-labs/inkathon#contracts

# Build all contracts
./build-all.sh
# Build single contract
cargo contract build --release --manifest-path azns_registry/Cargo.toml

# Run all tests
./test-all.sh
# Run tests for single contract
cargo test --manifest-path azns_registry/Cargo.toml
```

## Scripts

All TypeScript files in the `./scripts` can be run with the commands below. Most of the scripts assume, that all contracts are built via `./build-all.sh`.

```bash
# Prerequisites (Node, pnpm)

# Install dependencies (once)
pnpm i

# Start local development chain
pnpm node

# Run any script (from `./scripts`)
pnpm run script testContractStorage

# Run script on other chain
# IMPORTANT: Create a `.{chain}.env` environment file (gitignored) with the `ACCOUNT_URI` you want to use.
#       Chains must be a network-id from here: https://github.com/scio-labs/use-inkathon/blob/main/src/chains.ts.
CHAIN=alephzero-testnet pnpm run script testContractStorage
```

## Deployment

A full deployment of all contracts is handled by `./scripts/deploy.ts` and can be run via `pnpm run deploy`.

This is an example of how to build & deploy all contracts into an external directory (e.g. frontend repository):

```bash
DIR=../frontend/src/deployments pnpm run build # same as `./build-all.sh`
DIR=../frontend/src/deployments pnpm run deploy # same as `pnpm run script deploy`
# DIR=../frontend/src/deployments CHAIN=alephzero-testnet pnpm run deploy
```

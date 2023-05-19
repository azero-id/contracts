import { alephzeroTestnet, getSubstrateChain } from '@scio-labs/use-inkathon'
import * as dotenv from 'dotenv'
import { deployFeeCalculator } from './deploy/deployFeeCalculator'
import { deployMerkleVerifierWithWhitelist } from './deploy/deployMerkleVerifier'
import { deployNameChecker } from './deploy/deployNameChecker'
import { deployRegistry } from './deploy/deployRegistry'
import { addRegistryToRouter, deployRouter } from './deploy/deployRouter'
import { setContractAdmin } from './deploy/setContractAdmin'
import { addReservedNames } from './reservations/addReservedNames'
import { ContractDeployments } from './utils/ContractDeployments.type'
import { initPolkadotJs } from './utils/initPolkadotJs'
import { writeContractAddresses } from './utils/writeContractAddresses'

// Dynamic environment variables
const chainId = process.env.CHAIN || 'development'
dotenv.config({ path: `.env.${process.env.CHAIN || 'development'}` })

/**
 * Script that deploys all contracts and writes their addresses to files.
 *
 * Parameters:
 *  - `DIR`: Directory to read deploy files & write contract addresses to (optional, defaults to `./src/deployments`)
 *  - `CHAIN`: Chain ID (optional, defaults to `development`)
 *  - `ADMIN`: Address of contract admin (optional, defaults to caller)
 *  - `WHITELIST`: Path to .txt file with whitelisted addresses (optional)
 *  - `RESERVED_NAMES`: Path to .csv file with reserved names & addresses (optional)
 *
 * Example usage:
 *  - `CHAIN=alephzero-testnet ADMIN=5fei… WHITELIST=whitelist.txt RESERVED_NAMES=reserved-names.csv pr deploy`
 */
const main = async () => {
  const chain = getSubstrateChain(chainId)
  if (!chain) throw new Error(`Chain '${chainId}' not found`)
  const accountUri = process.env.ACCOUNT_URI || '//Alice'
  const initParams = await initPolkadotJs(chain, accountUri)

  // Deploy all contracts
  const nameChecker = await deployNameChecker(initParams)
  const feeCalculator = await deployFeeCalculator(initParams)
  const merkleVerifier = process.env.WHITELIST
    ? await deployMerkleVerifierWithWhitelist(initParams)
    : null
  const tlds = chain.network === alephzeroTestnet.network ? ['tzero'] : ['azero', 'a0']
  const baseUri =
    chain.network === alephzeroTestnet.network
      ? 'https://tzero.id/api/v1/metadata/'
      : 'https://azero.id/api/v1/metadata/'
  const registry = await deployRegistry(initParams, {
    nameCheckerAddress: nameChecker.address,
    feeCalculatorAddress: feeCalculator.address,
    merkleVerifierAddress: merkleVerifier?.address,
    tld: tlds[0],
    baseUri,
  })
  const router = await deployRouter(initParams)

  // Add registry to router
  await addRegistryToRouter(initParams, router.address, tlds, registry.address)

  // Write contract addresses to `{contract}/{network}.ts` files
  await writeContractAddresses(chain.network, {
    azns_name_checker: nameChecker,
    azns_fee_calculator: feeCalculator,
    azns_merkle_verifier: merkleVerifier,
    azns_registry: registry,
    azns_router: router,
  })
}

main()
  .catch((error) => {
    console.error(error)
    process.exit(1)
  })
  .finally(() => process.exit(0))

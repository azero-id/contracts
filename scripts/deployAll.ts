import { getSubstrateChain } from '@scio-labs/use-inkathon'
import * as dotenv from 'dotenv'
import { deployFeeCalculator } from './deploy/deployFeeCalculator'
import { deployMerkleVerifierWithWhitelist } from './deploy/deployMerkleVerifier'
import { deployNameChecker } from './deploy/deployNameChecker'
import { deployRegistry } from './deploy/deployRegistry'
import { addRegistryToRouter, deployRouter } from './deploy/deployRouter'
import { initPolkadotJs } from './utils/initPolkadotJs'
import { writeContractAddresses } from './utils/writeContractAddresses'

// Dynamic environment variables
const chainId = process.env.CHAIN || 'development'
dotenv.config({ path: `.env.${process.env.CHAIN || 'development'}` })

/**
 * Script that deploys all contracts and writes their addresses to files.
 */
const main = async () => {
  const chain = getSubstrateChain(chainId)
  if (!chain) throw new Error(`Chain '${chainId}' not found`)
  const accountUri = process.env.ACCOUNT_URI || '//Alice'
  const initParams = await initPolkadotJs(chain, accountUri)

  // Deploy all contracts
  const { address: nameCheckerAddress } = await deployNameChecker(initParams)
  const { address: feeCalculatorAddress } = await deployFeeCalculator(initParams)
  const { address: merkleVerifierAddress } = process.env.WHITELIST
    ? await deployMerkleVerifierWithWhitelist(initParams)
    : { address: null }
  const { address: registryAddress } = await deployRegistry(initParams, {
    nameCheckerAddress,
    feeCalculatorAddress,
    merkleVerifierAddress,
  })
  const { address: routerAddress } = await deployRouter(initParams)

  // Add registry to router
  await addRegistryToRouter(initParams, routerAddress, ['azero', 'a0'], registryAddress)

  // Write contract addresses to `{contract}/{network}.ts` files
  await writeContractAddresses(chain.network, {
    azns_name_checker: nameCheckerAddress,
    azns_fee_calculator: feeCalculatorAddress,
    azns_merkle_verifier: merkleVerifierAddress,
    azns_registry: registryAddress,
    azns_router: routerAddress,
  })
}

main()
  .catch((error) => {
    console.error(error)
    process.exit(1)
  })
  .finally(() => process.exit(0))

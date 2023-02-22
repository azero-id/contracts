import { getSubstrateChain } from '@scio-labs/use-inkathon'
import * as dotenv from 'dotenv'
import { deployFeeCalculator } from './deploy/deployFeeCalculator'
import { deployMerkleVerifier } from './deploy/deployMerkleVerifier'
import { deployNameChecker } from './deploy/deployNameChecker'
import { deployRegistry } from './deploy/deployRegistry'
import { initPolkadotJs } from './utils/initPolkadotJs'
import { writeContractAddresses } from './utils/writeContractAddresses'

// Dynamic environment variables
dotenv.config({ path: `.env.${process.env.CHAIN}` })

/**
 * Script that deploys all contracts and writes their addresses to files.
 */
const main = async () => {
  const chain = getSubstrateChain(process.env.CHAIN || 'development')
  if (!chain) throw new Error(`Chain '${process.env.CHAIN}' not found`)
  const accountUri = process.env.ACCOUNT_URI || '//Alice'
  const initParams = await initPolkadotJs(chain.rpcUrls, accountUri)

  // Deploy all contracts
  const { address: aznsNameCheckerAddress } = await deployNameChecker(initParams)
  const { address: aznsFeeCalculatorAddress } = await deployFeeCalculator(initParams)
  const { address: aznsMerkleVerifierAddress } = await deployMerkleVerifier(initParams)
  const { address: aznsRegistryAddress } = await deployRegistry(initParams, {
    aznsNameCheckerAddress,
    aznsFeeCalculatorAddress,
    aznsMerkleVerifierAddress,
  })

  // Write contract addresses to `{contract}/{network}.ts` files
  await writeContractAddresses(chain.network, {
    azns_name_checker: aznsNameCheckerAddress,
    azns_fee_calculator: aznsFeeCalculatorAddress,
    azns_merkle_verifier: aznsMerkleVerifierAddress,
    azns_registry: aznsRegistryAddress,
  })
}

main()
  .catch((error) => {
    console.error(error)
    process.exit(1)
  })
  .finally(() => process.exit(0))

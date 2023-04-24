import { alephzeroTestnet, getSubstrateChain } from '@scio-labs/use-inkathon'
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
  const nameChecker = await deployNameChecker(initParams)
  const feeCalculator = await deployFeeCalculator(initParams)
  const merkleVerifier = process.env.WHITELIST
    ? await deployMerkleVerifierWithWhitelist(initParams)
    : null
  const tlds = chain.network === alephzeroTestnet.network ? ['tzero'] : ['azero', 'a0']
  const registry = await deployRegistry(initParams, {
    nameCheckerAddress: nameChecker.address,
    feeCalculatorAddress: feeCalculator.address,
    merkleVerifierAddress: merkleVerifier?.address,
    tld: tlds[0],
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

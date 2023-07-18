import { alephzeroTestnet, getSubstrateChain } from '@scio-labs/use-inkathon'
import * as dotenv from 'dotenv'
import { deployFeeCalculator } from './deploy/deployFeeCalculator'
import { deployMerkleVerifierWithWhitelist } from './deploy/deployMerkleVerifier'
import { deployNameChecker } from './deploy/deployNameChecker'
import { deployRegistry } from './deploy/deployRegistry'
import { addRegistryToRouter, deployRouter } from './deploy/deployRouter'
import { setContractAdmins } from './deploy/setContractAdmin'
import { addReservations } from './reservations/addReservation'
import { getReservationsFromCSV } from './reservations/getReservationsFromCSV'
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
 *  - `RESERVATIONS`: Path to .csv file with reserved names & addresses
 *
 * Example usage:
 *  - `CHAIN=alephzero-testnet ADMIN=5fei… WHITELIST=whitelist.txt RESERVATIONS=reserved-names.csv pr deploy`
 */
const main = async () => {
  const chain = getSubstrateChain(chainId)
  if (!chain) throw new Error(`Chain '${chainId}' not found`)
  const accountUri = process.env.ACCOUNT_URI || '//Alice'
  const initParams = await initPolkadotJs(chain, accountUri)

  // Deploy all contracts
  const merkleVerifier = process.env.WHITELIST
    ? await deployMerkleVerifierWithWhitelist(initParams)
    : null
  const nameChecker = await deployNameChecker(initParams)
  const feeCalculator = await deployFeeCalculator(initParams)
  const tlds = chain.network === alephzeroTestnet.network ? ['tzero'] : ['azero', 'a0']
  const tld = tlds[0]
  const baseUri =
    chain.network === alephzeroTestnet.network
      ? 'https://tzero.id/api/v1/metadata/'
      : 'https://azero.id/api/v1/metadata/'
  const registry = await deployRegistry(initParams, {
    nameCheckerAddress: nameChecker.address,
    feeCalculatorAddress: feeCalculator.address,
    merkleVerifierAddress: merkleVerifier?.address,
    tld,
    baseUri,
  })
  const router = await deployRouter(initParams)

  // Map contract names to their deployment results
  const deployments: ContractDeployments = {
    azns_name_checker: nameChecker,
    azns_fee_calculator: feeCalculator,
    azns_merkle_verifier: merkleVerifier,
    azns_registry: registry,
    azns_router: router,
  }

  // Add reserved names to registry
  if (process.env.RESERVATIONS) {
    const reservations = await getReservationsFromCSV(initParams)
    await addReservations(initParams, registry.address, reservations)
  }

  // Add registry to router
  await addRegistryToRouter(initParams, router.address, tlds, registry.address)

  // Set new contract admins
  if (process.env.ADMIN) await setContractAdmins(initParams, deployments)

  // Write contract addresses to `{contract}/{network}.ts` files
  await writeContractAddresses(chain.network, deployments, { tld })
}

main()
  .catch((error) => {
    console.error(error)
    process.exit(1)
  })
  .finally(() => process.exit(0))

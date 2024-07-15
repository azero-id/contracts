import { alephzero, alephzeroTestnet, development } from '@scio-labs/use-inkathon'
import { deployFeeCalculator } from './deploy/deployFeeCalculator'
import { deployNameChecker } from './deploy/deployNameChecker'
import { deployRegistry } from './deploy/deployRegistry'
import { addRegistryToRouter, deployRouter } from './deploy/deployRouter'
import { setContractAdmins } from './deploy/setContractAdmin'
import { addReservations } from './reservations/addReservation'
import { getReservationsFromCSV } from './reservations/getReservationsFromCSV'
import { ContractDeployments } from './types/ContractDeployments.type'
import { initPolkadotJs } from './utils/initPolkadotJs'
import { writeContractAddresses } from './utils/writeContractAddresses'

/**
 * Script that deploys all contracts and writes their addresses to files.
 *
 * Parameters:
 *  - `DIR`: Directory to read contract build artifacts & write addresses to (optional, defaults to `./deployments`)
 *  - `CHAIN`: Chain ID (optional, defaults to `development`)
 *  - `ADMIN`: Address of contract admin (optional, defaults to caller)
 *  - `RESERVATIONS`: Path to .csv file with reserved names & addresses
 *
 * Example usage:
 *  - `CHAIN=alephzero-testnet ADMIN=5fei… RESERVATIONS=reserved-names.csv pnpm run deploy`
 */
const main = async () => {
  const initParams = await initPolkadotJs()
  const chainId = initParams.chain.network

  // Deploy all contracts
  const nameChecker = await deployNameChecker(initParams)
  const feeCalculator = await deployFeeCalculator(initParams)
  const tlds = chainId === alephzero.network ? ['azero', 'a0'] : ['tzero']
  const tld = tlds[0]
  const baseUri =
    {
      [alephzero.network]: 'https://azero.id/api/v1/metadata/',
      [alephzeroTestnet.network]: 'https://tzero.id/api/v1/metadata/',
      [development.network]: 'http://localhost:3000/api/v1/metadata/',
    }[chainId] || 'https://tzero.id/api/v1/metadata/'
  const registry = await deployRegistry(initParams, {
    nameCheckerAddress: nameChecker.address,
    feeCalculatorAddress: feeCalculator.address,
    tld,
    baseUri,
  })
  const router = await deployRouter(initParams)

  // Map contract names to their deployment results
  const deployments: ContractDeployments = {
    azns_name_checker: nameChecker,
    azns_fee_calculator: feeCalculator,
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
  await writeContractAddresses(chainId, deployments, { tld })
}

main()
  .catch((error) => {
    console.error(error)
    process.exit(1)
  })
  .finally(() => process.exit(0))

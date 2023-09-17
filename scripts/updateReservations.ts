import { ContractPromise } from '@polkadot/api-contract'
import { contractQuery, decodeOutput, getSubstrateChain } from '@scio-labs/use-inkathon'
import * as dotenv from 'dotenv'
import { Reservation } from './reservations/Reservation.type'
import { addReservations } from './reservations/addReservation'
import { getReservationsFromCSV } from './reservations/getReservationsFromCSV'
import { getDeploymentData } from './utils/getDeploymentData'
import { InitParams, initPolkadotJs } from './utils/initPolkadotJs'

// Dynamic environment variables
const chainId = process.env.CHAIN || 'development'
dotenv.config({ path: `.env.${process.env.CHAIN || 'development'}` })

/**
 * Script that loads reserved names from a given .csv file and adds them to the given registry contract.
 * No deployments are done.
 *
 * Parameters:
 *  - `REGISTRY_ADDRESS`: Address of registry contract
 *  - `RESERVATIONS`: Path to .csv file with reserved names & addresses
 *  - `DIR`: Directory to read deploy files & write contract addresses to (optional, defaults to `./src/deployments`)
 *  - `CHAIN`: Chain ID (optional, defaults to `development`)
 */
const main = async () => {
  const chain = getSubstrateChain(chainId)
  if (!chain) throw new Error(`Chain '${chainId}' not found`)
  const accountUri = process.env.ACCOUNT_URI || '//Alice'
  const derivationPath = process.env.ACCOUNT_DERIVATION_PATH || ''
  const initParams = await initPolkadotJs(chain, `${accountUri}${derivationPath}`)
  const { api } = initParams

  // Create registry contract instance
  const { abi } = await getDeploymentData('azns_registry')
  const registryAddress = process.env.REGISTRY_ADDRESS
  if (!registryAddress) throw new Error('No registry address provided')
  const contract = new ContractPromise(api, abi, registryAddress)

  // Load reserved names from .csv file
  const reservations = await getReservationsFromCSV(initParams)

  // Filter out already registered names
  const unregisteredReservations = await filterOutAlreadyRegisteredNames(
    initParams,
    contract,
    reservations,
  )

  // Add reserved names to registry contract
  await addReservations(initParams, registryAddress, unregisteredReservations)
}

/**
 * Helper that fetches the current name status from the registry contract
 * and filters out already registered names.
 */
const filterOutAlreadyRegisteredNames = async (
  { api }: InitParams,
  contract: ContractPromise,
  reservations: Reservation[],
) => {
  // Fetch current name status
  const reservedNames = reservations.map(([name]) => name)
  const response = await contractQuery(api, '', contract, 'get_name_status', {}, [reservedNames])
  const { isError, decodedOutput } = decodeOutput(response, contract, 'get_name_status')
  if (isError) throw new Error(`Error querying contract: ${decodedOutput}`)

  const namesStatus = response?.output?.toPrimitive()?.['ok']
  if (!namesStatus || !Array.isArray(namesStatus) || namesStatus.length !== reservedNames.length)
    throw new Error(`Invalid contract response: ${namesStatus}`)

  // Count name status & Filter out already registered names
  let [countReserved, countRegistered, countAvailable, countUnavailable] = [0, 0, 0, 0]
  const unregisteredReservations = []
  const registeredNames = []
  for (const idx in namesStatus) {
    const staus = Object.keys(namesStatus[idx])[0]
    const reservation = reservations[idx]

    // Increment status counts
    if (staus === 'registered') countRegistered++
    else if (staus === 'reserved') countReserved++
    else if (staus === 'available') countAvailable++
    else if (staus === 'unavailable') countUnavailable++

    if (staus === 'registered') {
      registeredNames.push(reservation[0])
    } else {
      // Add not-yet-registered names to list
      unregisteredReservations.push(reservation)
    }
  }
  console.log('\n', { countRegistered, countReserved, countAvailable, countUnavailable })
  console.log(`\nFiltered out ${countRegistered} already registered names:`, registeredNames)

  // Security check if counts add up
  const statusCounts = countReserved + countRegistered + countAvailable + countUnavailable
  if (statusCounts !== reservedNames.length)
    throw new Error(`Status counts (${statusCounts}) don't add up to ${reservedNames.length}`)

  return unregisteredReservations
}

main()
  .catch((error) => {
    console.error(error)
    process.exit(1)
  })
  .finally(() => process.exit(0))

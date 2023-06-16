import { ContractPromise } from '@polkadot/api-contract'
import { contractTx } from '@scio-labs/use-inkathon'
import { getDeploymentData } from '../utils/getDeploymentData'
import { InitParams } from '../utils/initPolkadotJs'
import { Reservation } from './Reservation.interface'
import { validateReservations } from './validateReservations'

/**
 * Validates & adds given reserved names to the registry contract.
 */
export const addReservations = async (
  initParams: InitParams,
  registryAddress: string,
  reservations: Reservation[],
) => {
  // Validate reservations
  const validatedReservations = await validateReservations(initParams, reservations)

  // Add reservations (names & addresses) to registry contract
  const { api, account } = initParams
  const { abi } = await getDeploymentData('azns_registry')
  const contract = new ContractPromise(api, abi, registryAddress)
  try {
    await contractTx(api, account, contract, 'add_reserved_names', {}, [validatedReservations])
    console.log(`\nSuccessfully added ${validatedReservations.length} reserved names to registry.`)
  } catch (error) {
    throw new Error('Error while adding reserved names to registry.')
  }
}

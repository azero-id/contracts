import { ContractPromise } from '@polkadot/api-contract'
import { contractTx } from '@scio-labs/use-inkathon'
import { getDeploymentData } from '../utils/getDeploymentData'
import { InitParams } from '../utils/initPolkadotJs'
import { Reservation } from './Reservation.type'
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
    // Add reservations to registry contract in batches
    const amountBatches = Math.ceil(validatedReservations.length / 100)
    for (let i = 0; i < amountBatches; i++) {
      const batch = validatedReservations.slice(i * 100, (i + 1) * 100)
      await contractTx(api, account, contract, 'add_reserved_names', {}, [batch])
      console.log(
        `Successfully added ${batch.length} reserved names to registry (batch ${
          i + 1
        }/${amountBatches}).`,
      )
    }
  } catch (error) {
    throw new Error('Error while adding reserved names to registry.')
  }
}

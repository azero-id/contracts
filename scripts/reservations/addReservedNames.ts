import { ContractPromise } from '@polkadot/api-contract'
import { contractTx } from '@scio-labs/use-inkathon'
import { getDeploymentData } from '../utils/getDeploymentData'
import { InitParams } from '../utils/initPolkadotJs'
import { getReservedNames } from './getReservedNames'

/**
 * Adds reserved names (from .csv) to the registry contract.
 */
export const addReservedNames = async (initParams: InitParams, registryAddress: string) => {
  const reservedNames = await getReservedNames(initParams, process.env.RESERVED_NAMES)

  const { api, account } = initParams
  const { abi } = await getDeploymentData('azns_registry')
  const contract = new ContractPromise(api, abi, registryAddress)
  try {
    await contractTx(api, account, contract, 'add_reserved_names', {}, [reservedNames])
    console.log(`\nSuccessfully added ${reservedNames.length} reserved names to registry.`)
  } catch (error) {
    throw new Error('Error while adding reserved names to registry.')
  }
}

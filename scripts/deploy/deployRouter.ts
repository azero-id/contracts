import { ContractPromise } from '@polkadot/api-contract'
import { contractTx, deployContract } from '@scio-labs/use-inkathon'
import { InitParams } from 'scripts/utils/initPolkadotJs'
import { DeployFn } from '../utils/DeployFn.type'
import { getDeploymentData } from '../utils/getDeploymentData'

/**
 * Deploys the `azns_router` contract
 */
export type RouterArgs = {
  admin: string
}
export const deployRouter: DeployFn<RouterArgs> = async ({ api, account }, customArgs) => {
  const args = Object.assign(
    {
      admin: account.address,
    } as RouterArgs,
    customArgs,
  )
  const { abi, wasm } = await getDeploymentData('azns_router')

  return await deployContract(api, account, abi, wasm, 'new', [args.admin])
}

/**
 * Adds a given registry contract to a router contract for a set of tlds.
 */
export const addRegistryToRouter = async (
  { api, account }: InitParams,
  routerAddress: string,
  tlds: string[],
  registryAddress: string,
) => {
  const { abi } = await getDeploymentData('azns_router')
  const contract = new ContractPromise(api, abi, routerAddress)
  try {
    await contractTx(api, account, contract, 'add_registry', {}, [tlds, registryAddress])
    console.log(
      `\nSuccessfully added registry ${registryAddress} to router for tlds: ${tlds.join(', ')}`,
    )
  } catch (error) {
    throw new Error('Error while adding registry to router.')
  }
}

import { deployContract } from '@scio-labs/use-inkathon'
import { DeployFn } from '../utils/DeployFn.type'
import { getDeploymentData } from '../utils/getDeploymentData'

/**
 * Deploys the `azns_router` contract
 */
export type RouterArgs = {
  admin: string
}
export const deployRouter: DeployFn<RouterArgs> = async (
  { api, account },
  customArgs,
) => {
  const args = Object.assign(
    {
      admin: account.address,
    } as RouterArgs,
    customArgs,
  )
  const { abi, wasm } = await getDeploymentData('azns_router')

  return await deployContract(api, account, abi, wasm, 'new', [args.admin])
}

import { deployContract } from '@scio-labs/use-inkathon'
import { DeployFn } from '../types/DeployFn.type'
import { getDeploymentData } from '../utils/getDeploymentData'

/**
 * Deploys the `azns_registry` contract
 */
export type RegistryArgs = {
  admin: string
  nameCheckerAddress: string
  feeCalculatorAddress: string
  tld: string
  baseUri: string
}
export const deployRegistry: DeployFn<RegistryArgs> = async ({ api, account }, customArgs) => {
  const args = Object.assign(
    {
      admin: account.address,
      nameCheckerAddress: null,
      feeCalculatorAddress: null,
      tld: 'azero',
      baseUri: 'https://azero.id/api/v1/metadata/',
    } as RegistryArgs,
    customArgs,
  )
  const { abi, wasm } = await getDeploymentData('azns_registry')
  const nonce = await api.rpc.system.accountNextIndex(account.address)

  return await deployContract(
    api,
    account,
    abi,
    wasm,
    'new',
    [args.admin, args.nameCheckerAddress, args.feeCalculatorAddress, args.tld, args.baseUri],
    undefined,
    { nonce },
  )
}

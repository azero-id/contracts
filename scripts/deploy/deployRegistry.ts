import { BN } from '@polkadot/util'
import { deployContract } from '@scio-labs/use-inkathon'
import { DeployFn } from '../utils/DeployFn.type'
import { getDeploymentData } from '../utils/getDeploymentData'

/**
 * Deploys the `azns_registry` contract
 */
export type RegistryArgs = {
  admin: string
  aznsNameCheckerAddress: string
  aznsFeeCalculatorAddress: string
  aznsMerkleVerifierAddress: string
  reservedNames: [string, string][] // [name, address | null][]
  tld: string
  metadataSizeLimit: BN
}
export const deployRegistry: DeployFn<RegistryArgs> = async ({ api, account }, customArgs) => {
  const args = Object.assign(
    {
      admin: account.address,
      aznsNameCheckerAddress: null,
      aznsFeeCalculatorAddress: null,
      aznsMerkleVerifierAddress: null,
      reservedNames: [],
      tld: 'azero',
      metadataSizeLimit: new BN(16_000),
    } as RegistryArgs,
    customArgs,
  )
  const { abi, wasm } = await getDeploymentData('azns_registry')

  return await deployContract(api, account, abi, wasm, 'new', [
    args.admin,
    args.aznsNameCheckerAddress,
    args.aznsFeeCalculatorAddress,
    args.aznsMerkleVerifierAddress,
    args.reservedNames,
    args.tld,
    args.metadataSizeLimit,
  ])
}

import { BN } from '@polkadot/util'
import { deployContract } from '@scio-labs/use-inkathon'
import { DeployFn } from '../utils/DeployFn.type'
import { getDeploymentData } from '../utils/getDeploymentData'

/**
 * Deploys the `azns_registry` contract
 */
export type RegistryArgs = {
  admin: string
  nameCheckerAddress: string
  feeCalculatorAddress: string
  merkleVerifierAddress: string
  reservedNames: [string, string][] // [name, address | null][]
  tld: string
  baseUri: string
  metadataSizeLimit: BN
}
export const deployRegistry: DeployFn<RegistryArgs> = async ({ api, account }, customArgs) => {
  const args = Object.assign(
    {
      admin: account.address,
      nameCheckerAddress: null,
      feeCalculatorAddress: null,
      merkleVerifierAddress: null,
      reservedNames: [],
      tld: 'azero',
      baseUri: 'https://dev.azero.domains/api/v1/metadata/',
      metadataSizeLimit: null,
    } as RegistryArgs,
    customArgs,
  )
  const { abi, wasm } = await getDeploymentData('azns_registry')

  return await deployContract(api, account, abi, wasm, 'new', [
    args.admin,
    args.nameCheckerAddress,
    args.feeCalculatorAddress,
    args.merkleVerifierAddress,
    args.reservedNames,
    args.tld,
    args.baseUri,
    args.metadataSizeLimit,
  ])
}

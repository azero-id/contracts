import { deployContract } from '@scio-labs/use-inkathon'
import { DeployFn } from '../utils/DeployFn.type'
import { getDeploymentData } from '../utils/getDeploymentData'

/**
 * Deploys the `azns_name_checker` contract
 */
export type NameCheckerArgs = {
  admin: string
  allowedLength: [number, number] // [min, max]
  allowedUnicodeRanges: [number, number][] // [start, end][]
  disallowedUnicodeRangesForEdges: [number, number][] // [start, end][]
}
export const deployNameChecker: DeployFn<NameCheckerArgs> = async (
  { api, account },
  customArgs,
) => {
  const args = Object.assign(
    {
      admin: account.address,
      allowedLength: [5, 64],
      allowedUnicodeRanges: [
        ['a'.charCodeAt(0), 'z'.charCodeAt(0)],
        ['0'.charCodeAt(0), '9'.charCodeAt(0)],
        ['-'.charCodeAt(0), '-'.charCodeAt(0)],
      ],
      disallowedUnicodeRangesForEdges: [['-'.charCodeAt(0), '-'.charCodeAt(0)]],
    } as NameCheckerArgs,
    customArgs,
  )
  const { abi, wasm } = await getDeploymentData('azns_name_checker')

  return await deployContract(api, account, abi, wasm, 'new', [
    args.admin,
    args.allowedLength,
    args.allowedUnicodeRanges,
    args.disallowedUnicodeRangesForEdges,
  ])
}

import { deployContract } from '@scio-labs/use-inkathon'
import { DeployFn } from '../utils/DeployFn.type'
import { getDeploymentData } from '../utils/getDeploymentData'

/**
 * Deploys the `azns_merkle_verifier` contract
 */
export type MerkleVerifierArgs = {
  root: Uint8Array
}
export const deployMerkleVerifier: DeployFn<MerkleVerifierArgs> = async (
  { api, account },
  customArgs,
) => {
  const args = Object.assign(
    {
      root: new Uint8Array(),
    } as MerkleVerifierArgs,
    customArgs,
  )
  const { abi, wasm } = await getDeploymentData('azns_merkle_verifier')

  return await deployContract(api, account, abi, wasm, 'new', [args.root])
}

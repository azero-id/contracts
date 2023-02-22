import { deployContract } from '@scio-labs/use-inkathon'
import { DeployFn } from '../utils/DeployFn.type'
import { getDeploymentData } from '../utils/getDeploymentData'
import { constructMerkleTree } from '../whitelist/constructMerkleTree'
import { getWhitelistAddresses } from '../whitelist/getWhitelistAddresses'

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

/**
 * Deploys the `azns_merkle_verifier` contract with a merkle-tree of fetched whitelist addresses.
 */
export const deployMerkleVerifierWithWhitelist: DeployFn<never> = async ({ api, account }) => {
  // Fetch whitelist addresses & construct merkle tree
  const addresses = await getWhitelistAddresses(process.env.WHITELIST)
  const { encodedRoot } = constructMerkleTree(addresses)

  const { abi, wasm } = await getDeploymentData('azns_merkle_verifier')

  return await deployContract(api, account, abi, wasm, 'new', [encodedRoot])
}

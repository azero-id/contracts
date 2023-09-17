import { deployContract } from '@scio-labs/use-inkathon'
import { DeployFn } from '../types/DeployFn.type'
import { getDeploymentData } from '../utils/getDeploymentData'
import { constructMerkleTree } from '../whitelist/constructMerkleTree'
import { getWhitelistAddresses } from '../whitelist/getWhitelistAddresses'

/**
 * Deploys the `azns_merkle_verifier` contract
 */
export type MerkleVerifierArgs = {
  admin: string
  root: Uint8Array
}
export const deployMerkleVerifier: DeployFn<MerkleVerifierArgs> = async (
  { api, account },
  customArgs,
) => {
  const args = Object.assign(
    {
      admin: account.address,
      root: new Uint8Array(),
    } as MerkleVerifierArgs,
    customArgs,
  )
  const { abi, wasm } = await getDeploymentData('azns_merkle_verifier')

  return await deployContract(api, account, abi, wasm, 'new', [args.admin, args.root])
}

/**
 * Deploys the `azns_merkle_verifier` contract with a merkle-tree of fetched whitelist addresses.
 */
export const deployMerkleVerifierWithWhitelist: DeployFn<Omit<MerkleVerifierArgs, 'root'>> = async (
  initParams,
  customArgs,
) => {
  const { api, account } = initParams

  // Fetch whitelist addresses & construct merkle tree
  const addresses = await getWhitelistAddresses(
    initParams,
    process.env.WHITELIST,
    !!process.env.SAVE_HASHED_WHITELIST,
  )
  const { encodedRoot } = constructMerkleTree(addresses)

  // Gather deployment params & data
  const args = Object.assign(
    {
      admin: account.address,
      root: encodedRoot,
    } as MerkleVerifierArgs,
    customArgs,
  )

  const { abi, wasm } = await getDeploymentData('azns_merkle_verifier')

  return await deployContract(api, account, abi, wasm, 'new', [args.admin, args.root])
}

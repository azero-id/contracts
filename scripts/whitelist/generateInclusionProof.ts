import { hexToU8a } from '@polkadot/util'
import MerkleTree from 'merkletreejs'
import { hashAccountId } from '../utils/hashAccountId'

/**
 * Generates the proof of inclusion of given accountId in the merkle tree
 * @param tree MerkleTree object on which the proof is constructed
 * @param accountId Item whose inclusion needs to be proved
 * @returns proof (Buffer[]) and verification result (boolean)
 */
export const generateInclusionProof = (tree: MerkleTree, accountId: string) => {
  const leaf = hashAccountId(accountId)
  const encodedLeaf = hexToU8a(leaf.toString())
  const proof = tree.getProof(leaf).map((x) => x.data)
  const verification = tree.verify(proof, leaf, tree.getRoot())

  return {
    encodedLeaf,
    proof,
    verification,
  }
}

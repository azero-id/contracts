import { bufferToU8a } from '@polkadot/util'
import sha3js from 'js-sha3'
import { MerkleTree } from 'merkletreejs'
import { hashAccountId } from '../utils/hashAccountId'

/**
 * Constructs the merkle tree from the given addresses (whitelisted accounts)
 * @param addresses Array of ss58 addresses
 * @returns MerkleTree object and encoded merkle root
 */
export const constructMerkleTree = async (addresses: string[]) => {
  console.log(`Constructing merkle tree with ${addresses.length} leaves (addresses)…`)

  // Convert addresses to hashes
  const addressLeaves = addresses.map((address) => hashAccountId(address))

  // Construct merkle tree
  const tree = new MerkleTree(addressLeaves, sha3js.keccak256, {
    sortPairs: true,
  })
  console.log('Merkle root (encoded):', tree.getHexRoot())

  // Encode merkle root
  const encodedRoot = bufferToU8a(tree.getRoot())

  return {
    tree,
    encodedRoot,
  }
}

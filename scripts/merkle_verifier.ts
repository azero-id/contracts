import { ApiPromise } from '@polkadot/api'
import { ContractPromise } from '@polkadot/api-contract'
import { IKeyringPair } from '@polkadot/types/types/interfaces'
import { bufferToU8a, hexToNumber, hexToU8a } from '@polkadot/util'
import { contractQuery, deployContract, getSubstrateChain } from '@scio-labs/use-inkathon'
import cryptojs from 'crypto-js'
import sha3js from 'js-sha3'
import { MerkleTree } from 'merkletreejs'
import { getDeploymentData } from './utils/getDeploymentData'
import { initPolkadotJs } from './utils/initPolkadotJs'

/**
 * Checks on-chain Proofs work
 * @param tree MerkleTree object on which the proof is constructed
 * @param contractAddress MerkleVerifier contract address
 * @param item Item whose inclusion needs to be verifed
 */
const verifyProofs = async (
  api: ApiPromise,
  account: IKeyringPair,
  abi: any,
  tree: MerkleTree,
  contractAddress: string,
  item: string,
) => {
  const leaf = cryptojs.SHA256(item)
  const leaf_encoded = hexToU8a(leaf.toString())
  const proof = tree.getProof(leaf).map((x) => x.data)

  // Query contract
  const contract = new ContractPromise(api, abi, contractAddress)
  const { result, output } = await contractQuery(
    api,
    account.address,
    contract,
    'verify_proof',
    {},
    [leaf_encoded, proof],
  )

  if (result.isOk) {
    const isTrue = hexToNumber(result.asOk.data.toHex()) == 1
    console.log('On-chain verification:', isTrue)
  } else {
    console.error('Error while querying contract (verify_proof). Got result:', result)
  }
}

async function main() {
  const accountUri = process.env.ACCOUNT_URI || '//Alice'
  const chain = getSubstrateChain(process.env.CHAIN || 'development')
  if (!chain) throw new Error(`Chain '${process.env.CHAIN}' not found`)

  const { api, account } = await initPolkadotJs(chain.rpcUrls, accountUri)

  // 1. Construct MerkleTree
  const leaves = ['a', 'b', 'c'].map((x) => cryptojs.SHA256(x))
  const tree = new MerkleTree(leaves, sha3js.keccak256, {
    sortPairs: true,
  })
  console.log('Merkle root:', tree.getHexRoot())
  const root_encoded = bufferToU8a(tree.getRoot())

  // 2. Deploy contract
  let { abi, wasm } = await getDeploymentData('merkle_verifier')
  const { address } = await deployContract(api, account, abi, wasm, 'new', [root_encoded])

  // 3. Verify Proofs
  const leaf = cryptojs.SHA256('a')
  const proof = tree.getHexProof(leaf)
  const badLeaves = ['a', 'x', 'c'].map((x) => cryptojs.SHA256(x))
  const badTree = new MerkleTree(badLeaves, cryptojs.SHA256)
  const badLeaf = cryptojs.SHA256('x')
  const badProof = badTree.getProof(badLeaf)

  await verifyProofs(api, account, abi, tree, address, 'b')
}

main()
  .catch((error) => {
    console.error(error)
    process.exit(1)
  })
  .finally(() => process.exit(0))

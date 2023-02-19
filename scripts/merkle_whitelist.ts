import { ApiPromise, Keyring } from '@polkadot/api'
import { ContractPromise } from '@polkadot/api-contract'
import { EventRecord } from '@polkadot/types/interfaces'
import { IKeyringPair } from '@polkadot/types/types/interfaces'
import { bufferToU8a, u8aToHex } from '@polkadot/util'
import {
  contractQuery,
  contractTx,
  deployContract,
  getSubstrateChain,
} from '@scio-labs/use-inkathon'
import BN from 'bn.js'
import cryptojs from 'crypto-js'
import sha3js from 'js-sha3'
import { MerkleTree } from 'merkletreejs'
import { getDeploymentData } from './utils/getDeploymentData'
import { initPolkadotJs } from './utils/initPolkadotJs'

/**
 * Parses & Hashes the accountId
 * @param accountId SS58 encoded AccountId
 * @returns SHA256(accountId)
 */
const hashAccountId = (accountId: string) => {
  const keyring = new Keyring({ type: 'sr25519' })
  const pubkey = u8aToHex(keyring.decodeAddress(accountId))
  const hexkey = cryptojs.enc.Hex.parse(pubkey.slice(2))
  return cryptojs.SHA256(hexkey)
}

/**
 * Constructs the merkle tree from the whitelisted accounts
 * @returns MerkleTree Object
 */
const constructMerkleTree = async (account: IKeyringPair) => {
  const leaves = [
    '5Gju41fG3iX4ZgYP8qYeJgNntSaAXYdh84F6pa1nVxCgVibu',
    '5E56jqWxmhdnuUy6RJsar2Uf89FjUDtCKRTEFcf5SyyvoZJg',
    '5CcBFjse1bTp1eeFUR5sAjxVQm4nuD3vgtJZy6p3iFj4ae63',
    account.address,
  ].map((addr) => hashAccountId(addr))

  const tree = new MerkleTree(leaves, sha3js.keccak256, {
    sortPairs: true,
  })

  console.log('Merkle root:', tree.getHexRoot())
  return tree
}

/**
 *
 * @param tree MerkleTree object on which the proof is constructed
 * @param accountId Item whose inclusion needs to be proved
 * @returns Buffer[] - proof of the inclusion of given accountId in the merkle tree
 */
const generateProof = async (tree: MerkleTree, accountId: string) => {
  const leaf = hashAccountId(accountId)
  const proof = tree.getProof(leaf).map((x) => x.data)

  console.log('Off-chain verification:', tree.verify(proof, leaf, tree.getRoot()))
  return proof
}

/**
 * Registers a domain by a whitelisted account
 * @param contractAddress `azns_registry` deployed address
 * @param signer Account which will sign the tx
 * @param domain Domain name to be registered
 * @param price The price user is willing to pay
 * @param proof proof of whitelist of given accountId
 */
const registerWithProof = async (
  api: ApiPromise,
  account: IKeyringPair,
  contract: ContractPromise,
  domain: string,
  price: BN,
  proof: Buffer[],
) => {
  // Check the proof is working on-chain
  const { result, output } = await contractQuery(
    api,
    account.address,
    contract,
    'verify_proof',
    {},
    [account.address, proof],
  )

  if (result.isOk && !!output) {
    const res = output.toPrimitive()['ok']
    console.log('On-chain verification:', !!res)
  } else {
    console.error('Error while querying contract (verify_proof). Got result:', result)
  }

  // Register domain with proof
  const txCallback = ({ status, events }) => {
    const failedEvent: EventRecord = events.find(
      ({ event: { method } }: any) => method === 'ExtrinsicFailed',
    )
    if (failedEvent) {
      console.error(`Domain couldn't be registered:`, failedEvent?.event?.data?.toHuman())
    } else if (status?.isInBlock) {
      console.log(`Registered domain '${domain}.azero' successfully`)
    }
  }
  await contractTx(
    api,
    account,
    contract,
    'register',
    { value: price },
    [domain, 1, null, proof, false],
    txCallback,
  )
  await new Promise((r) => setTimeout(r, 2000))
}

async function main() {
  const accountUri = process.env.ACCOUNT_URI || '//Alice'
  const chain = getSubstrateChain(process.env.CHAIN || 'development')
  if (!chain) throw new Error(`Chain '${process.env.CHAIN}' not found`)

  const { api, account, decimals } = await initPolkadotJs(chain.rpcUrls, accountUri)
  const decimalsMul = new BN(10 ** decimals)

  // 1. Construct merkle tree
  const tree = await constructMerkleTree(account)
  const root_encoded = bufferToU8a(tree.getRoot())

  // 2. Deploy all contracts
  let { abi, wasm } = await getDeploymentData('azns_name_checker')
  const allowedLength = [5, 64]
  const allowedUnicodeRanges = [['a'.charCodeAt(0), 'z'.charCodeAt(0)]]
  const { hash: aznsNameCheckerHash } = await deployContract(api, account, abi, wasm, 'new', [
    allowedLength,
    allowedUnicodeRanges,
    [],
  ])

  ;({ abi, wasm } = await getDeploymentData('azns_fee_calculator'))
  const price = new BN(6).mul(decimalsMul)
  const { hash: aznsFeeCalculatorHash } = await deployContract(api, account, abi, wasm, 'new', [
    account.address,
    3,
    price,
    [],
  ])

  ;({ abi, wasm } = await getDeploymentData('merkle_verifier'))
  const { hash: aznsMerkleVerifierHash } = await deployContract(api, account, abi, wasm, 'new', [
    root_encoded,
  ])

  ;({ abi, wasm } = await getDeploymentData('azns_registry'))
  const { address: aznsRegistryAddress } = await deployContract(api, account, abi, wasm, 'new', [
    aznsNameCheckerHash,
    null, // TODO
    aznsMerkleVerifierHash,
    root_encoded,
    [],
    1,
    allowedLength,
    allowedUnicodeRanges,
    [],
  ])

  // 3. Generate proof of a whitelisted address
  const proof = await generateProof(tree, account.address)

  // 4. Verify proof on-chain & Register domain
  const domain = 'alice'
  const contract = new ContractPromise(api, abi, aznsRegistryAddress)
  await registerWithProof(api, account, contract, domain, price, proof)
}

main()
  .catch((error) => {
    console.error(error)
    process.exit(1)
  })
  .finally(() => process.exit(0))

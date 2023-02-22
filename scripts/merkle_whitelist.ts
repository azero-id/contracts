import { ApiPromise, Keyring } from '@polkadot/api'
import { ContractPromise } from '@polkadot/api-contract'
import { IKeyringPair } from '@polkadot/types/types/interfaces'
import { bufferToU8a, hexToNumber, u8aToHex } from '@polkadot/util'
import { contractQuery, deployContract, getSubstrateChain } from '@scio-labs/use-inkathon'
import BN from 'bn.js'
import cryptojs from 'crypto-js'
import * as dotenv from 'dotenv'
import { existsSync } from 'fs'
import { open } from 'fs/promises'
import sha3js from 'js-sha3'
import { MerkleTree } from 'merkletreejs'
import path from 'path'
import { contractTxPromise } from './utils/contractTxPromise'
import { getDeploymentData } from './utils/getDeploymentData'
import { initPolkadotJs } from './utils/initPolkadotJs'
dotenv.config({ path: `.env.${process.env.CHAIN}` })

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
  let addressLeaves = [
    hashAccountId(account.address), // For testing
  ]

  // Fetch whitelisted accounts
  if (process.env.WHITELIST) {
    const whitelistFilePath = path.join(path.resolve(), process.env.WHITELIST)
    if (!existsSync(whitelistFilePath)) {
      throw new Error(`Whitelist file not found at ${whitelistFilePath}`)
    }
    addressLeaves = []
    const whitelistFile = await open(whitelistFilePath)
    for await (const address of whitelistFile.readLines()) {
      addressLeaves.push(hashAccountId(address))
    }
  }

  console.log('Number of leaves (addresses):', addressLeaves.length)
  const tree = new MerkleTree(addressLeaves, sha3js.keccak256, {
    sortPairs: true,
  })

  console.log('Merkle root:', tree.getHexRoot())
  return tree
}

/**
 * Generates the proof of inclusion of given accountId in the merkle tree
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
 * Registers a domain by a whitelisted account.
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
  const { result } = await contractQuery(api, account.address, contract, 'verify_proof', {}, [
    account.address,
    proof,
  ])

  if (result.isOk) {
    const isTrue = hexToNumber(result.asOk.data.toHex()) == 1
    console.log('On-chain verification:', isTrue)
  } else {
    console.error('Error while querying contract (verify_proof). Got result:', result)
  }

  // Register domain with proof
  try {
    await contractTxPromise(api, account, contract, 'register', { value: price.mul(new BN(3)) }, [
      domain,
      2,
      null,
      proof,
      false,
    ])
    console.log(`Registered domain '${domain}.azero' successfully`)
  } catch (e) {
    console.error(`Error while registering domain:`, e?.failedEvent?.event?.data?.toHuman())
  }
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
  const price = new BN(5).mul(decimalsMul)
  const allowedYears = 3
  const { address: aznsFeeCalculatorAddress } = await deployContract(
    api,
    account,
    abi,
    wasm,
    'new',
    [account.address, allowedYears, price, []],
  )

  ;({ abi, wasm } = await getDeploymentData('merkle_verifier'))
  const { hash: aznsMerkleVerifierHash } = await deployContract(api, account, abi, wasm, 'new', [
    root_encoded,
  ])

  ;({ abi, wasm } = await getDeploymentData('azns_registry'))
  const { address: aznsRegistryAddress } = await deployContract(api, account, abi, wasm, 'new', [
    aznsNameCheckerHash,
    aznsFeeCalculatorAddress,
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

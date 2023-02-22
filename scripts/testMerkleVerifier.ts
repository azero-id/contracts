import { ContractPromise } from '@polkadot/api-contract'
import { hexToNumber } from '@polkadot/util'
import { contractQuery, getSubstrateChain } from '@scio-labs/use-inkathon'
import * as dotenv from 'dotenv'
import { deployMerkleVerifier } from './deploy/deployMerkleVerifier'
import { getDeploymentData } from './utils/getDeploymentData'
import { initPolkadotJs } from './utils/initPolkadotJs'
import { constructMerkleTree } from './whitelist/constructMerkleTree'
import { generateInclusionProof } from './whitelist/generateInclusionProof'

// Dynamic environment variables
dotenv.config({ path: `.env.${process.env.CHAIN}` })

/**
 * Script that tests merkle tree verification off- and on-chain.
 */
const main = async () => {
  const chain = getSubstrateChain(process.env.CHAIN || 'development')
  if (!chain) throw new Error(`Chain '${process.env.CHAIN}' not found`)
  const accountUri = process.env.ACCOUNT_URI || '//Alice'
  const initParams = await initPolkadotJs(chain.rpcUrls, accountUri)
  const { api, keyring, account } = initParams

  // Sample addresses
  const includedAddress = account.address
  const notIncludedAddress = keyring.addFromUri('//Bob').address

  // Construct Merkle Tree
  const { tree, encodedRoot } = await constructMerkleTree([includedAddress])

  // Generate inclusion leafs and inclusion proofs
  const {
    encodedLeaf: encodedLeafA,
    proof: proofA,
    verification: verificationA,
  } = generateInclusionProof(tree, includedAddress)
  const {
    encodedLeaf: encodedLeafB,
    proof: proofB,
    verification: verificationB,
  } = generateInclusionProof(tree, notIncludedAddress)

  // Off-chain verification results
  const verificationOk = verificationA === true && verificationB === false
  console.log('Off-chain verifications:', verificationOk ? '✅' : '❌')

  // Deploy merkle-verifier contract
  const { address } = await deployMerkleVerifier(initParams, { root: encodedRoot })

  // Create ContractPromise and query contract (verify_proof)
  const { abi } = await getDeploymentData('azns_merkle_verifier')
  const contract = new ContractPromise(api, abi, address)
  const { result: resultA } = await contractQuery(api, address, contract, 'verify_proof', {}, [
    encodedLeafA,
    proofA,
  ])
  const { result: resultB } = await contractQuery(api, address, contract, 'verify_proof', {}, [
    encodedLeafB,
    proofB,
  ])

  // On-chain verification results
  if (resultA.isOk && resultB.isOk) {
    const verificationA = hexToNumber(resultA.asOk.data.toHex()) == 1
    const verificationB = hexToNumber(resultB.asOk.data.toHex()) == 1
    const verificationOk = verificationA === true && verificationB === false
    console.log('On-chain verifications:', verificationOk ? '✅' : '❌')
  } else {
    console.error('Error while querying contract (verify_proof). Got result:', resultA, resultB)
  }
}

main()
  .catch((error) => {
    console.error(error)
    process.exit(1)
  })
  .finally(() => process.exit(0))

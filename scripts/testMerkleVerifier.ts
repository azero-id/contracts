import { ContractPromise } from '@polkadot/api-contract'
import { contractQuery, getSubstrateChain } from '@scio-labs/use-inkathon'
import * as dotenv from 'dotenv'
import { deployMerkleVerifier } from './deploy/deployMerkleVerifier'
import { getDeploymentData } from './utils/getDeploymentData'
import { initPolkadotJs } from './utils/initPolkadotJs'
import { constructMerkleTree } from './whitelist/constructMerkleTree'
import { generateInclusionProof } from './whitelist/generateInclusionProof'
import { getWhitelistAddresses } from './whitelist/getWhitelistAddresses'

// Dynamic environment variables
const chainId = process.env.CHAIN || 'development'
dotenv.config({ path: `.env.${process.env.CHAIN || 'development'}` })

/**
 * Script that tests merkle tree verification off- and on-chain.
 *
 * Parameters:
 *  - `CHAIN`: Chain ID (optional, defaults to `development`)
 *  - `VERIFIER_ADDRESS`: Address of already deployed merkle-verifier contract (optional, defaults to deploying a new one)
 *  - `WHITELIST`: Path to .txt file with whitelisted addresses (optional)
 *  - `CHECK_ADDRESS`: Address to check & verify inclusion for (optional)
 *
 * Example usage:
 *  - `CHAIN=alephzero-testnet WHITELIST=whitelist.txt CHECK_ADDRESS=5fei… pnpm ts-node scripts/testMerkleVerifier.ts
 */
const main = async () => {
  const chain = getSubstrateChain(chainId)
  if (!chain) throw new Error(`Chain '${chainId}' not found`)
  const accountUri = process.env.ACCOUNT_URI || '//Alice'
  const initParams = await initPolkadotJs(chain, accountUri)
  const { api, keyring, account } = initParams

  // Addresses to check for
  const checkAddresses: [string, boolean][] = process.env.CHECK_ADDRESS
    ? [[process.env.CHECK_ADDRESS, true]]
    : [
        [account.address, true], // Example included address
        [keyring.addFromUri('//Bob').address, false], // Example not-included address
      ]

  // Construct Merkle Tree
  const addresses = process.env.WHITELIST
    ? await getWhitelistAddresses(
        initParams,
        process.env.WHITELIST,
        !!process.env.SAVE_HASHED_WHITELIST,
      )
    : [account.address] // Example tree, only with included address
  const { tree, encodedRoot } = constructMerkleTree(addresses)

  // Generate inclusion leafs and inclusion proofs
  const offChainProofResults: ReturnType<typeof generateInclusionProof>[] = checkAddresses.map(
    ([address]) => generateInclusionProof(tree, address),
  )

  // Off-chain verification results
  const offChainOk = offChainProofResults.every(
    (result, index) => result.verification === checkAddresses[index][1],
  )
  console.log('Off-chain verifications:', offChainOk ? '✅' : '❌')

  // Deploy merkle-verifier contract
  let verifierAddress = process.env.VERIFIER_ADDRESS
  if (!verifierAddress) {
    const { address } = await deployMerkleVerifier(initParams, { root: encodedRoot })
    verifierAddress = address
  }

  // Create ContractPromise and query contract (verify_proof)
  const { abi } = await getDeploymentData('azns_merkle_verifier')
  const contract = new ContractPromise(api, abi, verifierAddress)
  const onChainProofResults: boolean[] = await Promise.all(
    offChainProofResults.map(async ({ encodedLeaf, proof }) => {
      const { result, output } = await contractQuery(
        api,
        verifierAddress,
        contract,
        'verify_proof',
        {},
        [encodedLeaf, proof],
      )
      if (result.isOk) return output.toPrimitive()?.['ok']

      console.error(result)
      throw new Error(`Error while querying contract (verify_proof).`)
    }),
  )

  // On-chain verification results
  const onChainOk = onChainProofResults.every(
    (result, index) => result === checkAddresses[index][1],
  )
  console.log('On-chain verifications:', onChainOk ? '✅' : '❌')
}

main()
  .catch((error) => {
    console.error(error)
    process.exit(1)
  })
  .finally(() => process.exit(0))

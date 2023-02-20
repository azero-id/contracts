import { BN } from '@polkadot/util'
import { deployContract, getSubstrateChain } from '@scio-labs/use-inkathon'
import * as dotenv from 'dotenv'
import { getDeploymentData } from './utils/getDeploymentData'
import { initPolkadotJs } from './utils/initPolkadotJs'
import { writeContractAddresses } from './utils/writeContractAddresses'
dotenv.config({ path: `.env.${process.env.CHAIN}` })

const main = async () => {
  const accountUri = process.env.ACCOUNT_URI || '//Alice'
  const chain = getSubstrateChain(process.env.CHAIN || 'development')
  if (!chain) throw new Error(`Chain '${process.env.CHAIN}' not found`)

  const { api, account, decimals } = await initPolkadotJs(chain.rpcUrls, accountUri)
  const decimalsMul = new BN(10 ** decimals)

  // Deploy `azns_name_checker` contract
  let { abi, wasm } = await getDeploymentData('azns_name_checker')
  const allowedLength = [5, 64]
  const allowedUnicodeRanges = [
    ['a'.charCodeAt(0), 'z'.charCodeAt(0)],
    ['0'.charCodeAt(0), '9'.charCodeAt(0)],
    ['-'.charCodeAt(0), '-'.charCodeAt(0)],
  ]
  const disallowedUnicodeRangesForEdges = [['-'.charCodeAt(0), '-'.charCodeAt(0)]]
  const { address: aznsNameCheckerAddress, hash: aznsNameCheckerHash } = await deployContract(
    api,
    account,
    abi,
    wasm,
    'new',
    [allowedLength, allowedUnicodeRanges, disallowedUnicodeRangesForEdges],
  )

  // Deploy `azns_fee_calculator` contract
  ;({ abi, wasm } = await getDeploymentData('azns_fee_calculator'))
  const veryHighFee = new BN(1_000_000).mul(decimalsMul)
  const { address: aznsFeeCalculatorAddress, hash: aznsFeeCalculatorHash } = await deployContract(
    api,
    account,
    abi,
    wasm,
    'new',
    [
      account.address,
      3,
      6 * 10 ** decimals,
      [
        [1, veryHighFee],
        [2, veryHighFee],
        [3, veryHighFee],
        [4, veryHighFee],
      ],
    ],
  )

  // Deploy `merkle_verifier` contract
  ;({ abi, wasm } = await getDeploymentData('merkle_verifier'))
  const { address: aznsMerkleVerifierAddress, hash: aznsMerkleVerifierHash } = await deployContract(
    api,
    account,
    abi,
    wasm,
    'new',
    [[]],
  )

  // Deploy `azns_registry` contract
  ;({ abi, wasm } = await getDeploymentData('azns_registry'))
  const { address: aznsRegistryAddress, hash: aznsRegistryHash } = await deployContract(
    api,
    account,
    abi,
    wasm,
    'new',
    [
      aznsNameCheckerHash,
      null, // TODO
      aznsMerkleVerifierHash,
      [],
      [['dennis', null]],
      1,
      allowedLength,
      allowedUnicodeRanges,
      disallowedUnicodeRangesForEdges,
    ],
  )

  // Write contract addresses to `{contract}/{network}.ts` files
  await writeContractAddresses(chain.network, {
    azns_name_checker: aznsNameCheckerAddress,
    azns_fee_calculator: aznsFeeCalculatorAddress,
    merkle_verifier: aznsMerkleVerifierAddress,
    azns_registry: aznsRegistryAddress,
  })
}

main()
  .catch((error) => {
    console.error(error)
    process.exit(1)
  })
  .finally(() => process.exit(0))

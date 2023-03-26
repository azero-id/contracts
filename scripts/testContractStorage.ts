import { faker } from '@faker-js/faker'
import { ContractPromise } from '@polkadot/api-contract'
import { BN } from '@polkadot/util'
import { contractTx, getBalance, getSubstrateChain } from '@scio-labs/use-inkathon'
import * as cliProgress from 'cli-progress'
import * as dotenv from 'dotenv'
import { deployFeeCalculator } from './deploy/deployFeeCalculator'
import { deployNameChecker } from './deploy/deployNameChecker'
import { deployRegistry } from './deploy/deployRegistry'
import { getDeploymentData } from './utils/getDeploymentData'
import { initPolkadotJs } from './utils/initPolkadotJs'

// Dynamic environment variables
dotenv.config({ path: `.env.${process.env.CHAIN}` })

/**
 * Script that tests merkle tree verification off- and on-chain.
 *
 * Example:
 *  `DOMAIN_COUNT=10 METADATA_ROW_COUNT=10 METADATA_ITEM_SIZE=32 pnpm ts-node scripts/testContractStorage.ts`
 */
const main = async () => {
  const chain = getSubstrateChain(process.env.CHAIN || 'development')
  if (!chain) throw new Error(`Chain '${process.env.CHAIN}' not found`)
  const accountUri = process.env.ACCOUNT_URI || '//Alice'
  const initParams = await initPolkadotJs(chain, accountUri)
  const { api, decimals, account } = initParams

  // Deploy all contracts
  const { address: nameCheckerAddress } = await deployNameChecker(initParams)
  const { address: feeCalculatorAddress } = await deployFeeCalculator(initParams, {
    commonPrice: new BN(0),
  })
  const metadataSizeLimit = new BN(parseInt(process.env.METADATA_SIZE_LIMIT ?? '8000'))
  const { address: registryAddress } = await deployRegistry(initParams, {
    nameCheckerAddress,
    feeCalculatorAddress,
    merkleVerifierAddress: null,
    metadataSizeLimit,
  })

  // Create registry contract instance
  const { abi } = await getDeploymentData('azns_registry')
  const contract = new ContractPromise(api, abi, registryAddress)

  // Track gas usage
  const { balance: balanceStart } = await getBalance(api, account.address, 5)

  // Determine number of domains to register & amount of metadata to add
  const DOMAIN_COUNT = parseInt(process.env.DOMAIN_COUNT ?? '1000')
  const META_COUNT = parseInt(process.env.METADATA_ROW_COUNT ?? '5')
  const META_SIZE = parseInt(process.env.METADATA_ITEM_SIZE ?? '32')
  console.log(
    `\nRegistering ${DOMAIN_COUNT} domain(s) ${
      META_COUNT
        ? `with ${META_COUNT} metadata row(s) à ${2 * META_SIZE} characters (~${
            2 * META_SIZE * META_COUNT
          } bytes)`
        : 'without metadata'
    } …`,
  )

  // Progress bar
  let bar = new cliProgress.SingleBar({}, cliProgress.Presets.shades_classic)
  bar.start(DOMAIN_COUNT, 0)

  // Transactions
  for (let i = 0; i < DOMAIN_COUNT; i++) {
    try {
      // Register domain
      const domainName = faker.datatype.uuid()
      await registerDomain(api, account, contract, domainName)

      // Add metadata to domain
      await setDomainSampleMetadata(api, account, contract, domainName, META_COUNT, META_SIZE)
    } catch (e) {
      break
    }

    bar.update(i + 1)
  }
  bar.stop()

  // Output gas usage
  const { balance: balanceEnd, tokenSymbol } = await getBalance(api, account.address, 5)
  const balanceDiff = balanceStart.sub(balanceEnd)
  const balanceDiffFormatted =
    balanceDiff?.div?.(new BN(10).pow(new BN(decimals - 3))).toNumber() / 1000
  console.log(`\nGas Usage: ${balanceDiffFormatted} ${tokenSymbol}`)
}

/**
 * Helper function to register a domain.
 */
const registerDomain = async (api, account, contract, domainName) => {
  try {
    await contractTx(api, account, contract, 'register', { value: new BN(0) }, [
      domainName,
      1,
      null,
      null,
      false,
    ])
  } catch (e) {
    console.error(`Error while registering '${domainName}.azero':`, e)
    throw new Error()
  }
}

/**
 * Helper function to add sample metadata to a domain.
 */
const setDomainSampleMetadata = async (api, account, contract, domainName, rowCount, itemSize) => {
  if (!rowCount || !itemSize) return

  try {
    const sampleMetadata = new Array(rowCount)
      .fill(null)
      .map(() => [faker.datatype.string(itemSize), faker.datatype.string(itemSize)])
    await contractTx(api, account, contract, 'setAllRecords', {}, [domainName, sampleMetadata])
  } catch (e) {
    console.error(`Error while adding metadata to '${domainName}.azero':`, e)
    throw new Error()
  }
}

main()
  .catch((error) => {
    console.error(error)
    process.exit(1)
  })
  .finally(() => process.exit(0))

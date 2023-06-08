import { faker } from '@faker-js/faker'
import { ContractPromise } from '@polkadot/api-contract'
import { BN } from '@polkadot/util'
import {
  contractTx,
  getBalance,
  getSubstrateChain,
  transferBalance,
  transferFullBalance,
} from '@scio-labs/use-inkathon'
import * as cliProgress from 'cli-progress'
import * as dotenv from 'dotenv'
import { deployFeeCalculator } from './deploy/deployFeeCalculator'
import { deployNameChecker } from './deploy/deployNameChecker'
import { deployRegistry } from './deploy/deployRegistry'
import { getDeploymentData } from './utils/getDeploymentData'
import { initPolkadotJs } from './utils/initPolkadotJs'

// Dynamic environment variables
const chainId = process.env.CHAIN || 'development'
dotenv.config({ path: `.env.${process.env.CHAIN || 'development'}` })

/**
 * Script that tests different storage limits of the `azns_registry` contract.
 *
 * Example #1: `DOMAIN_COUNT=1000 pnpm ts-node scripts/testContractStorage.ts`
 *   - Tests limitations of reverse-mappings by creating 1000 domains for the same account.
 *   - Fails after 441 domains on Aleph Zero testnet due to storage limitations, see #75.
 *
 * Example #2: `DOMAIN_COUNT=1000 USE_RANDOM_ACCOUNTS=true pnpm ts-node scripts/testContractStorage.ts`
 *   - Should run through as it creates & funds a new random account for each domain.
 *
 * Example #3: `DOMAIN_COUNT=1 RECORDS_SIZE_LIMIT=8000 RECORD_ROW_COUNT=120 RECORDS_ITEM_SIZE=32 pnpm ts-node scripts/testContractStorage.ts`
 *   - Tests record size limits by creating a domain with 120 record rows with 64 characters each (32 key & 32 value).
 *   - Fails with lower `RECORDS_SIZE_LIMIT` or higher `RECORDS_ROW_COUNT` or `RECORDS_ITEM_SIZE` (2×32×120 ⪅ 8000 bytes).
 *
 * Example #4: `REGISTRY_ADDRESS=5fei… DIR=../indexer/src/deployments CHAIN=alephzero-testnet DOMAIN_PRICE=6 DOMAIN_COUNT=10 pnpm ts-node scripts/testContractStorage.ts`
 *   - Uses a previously deployed registry contract at a different directory with given domain price.
 */
const main = async () => {
  const chain = getSubstrateChain(chainId)
  if (!chain) throw new Error(`Chain '${chainId}' not found`)
  const accountUri = process.env.ACCOUNT_URI || '//Alice'
  const initParams = await initPolkadotJs(chain, accountUri)
  const { api, decimals, toBNWithDecimals, account, keyring } = initParams

  // Determine registry address
  let registryAddress = process.env.REGISTRY_ADDRESS || null
  const domainPrice: any = process.env.DOMAIN_PRICE
    ? toBNWithDecimals(process.env.DOMAIN_PRICE)
    : new BN(1)

  // Deploy all contracts if no registry address is provided
  if (!registryAddress) {
    const { address: nameCheckerAddress } = await deployNameChecker(initParams)
    const { address: feeCalculatorAddress } = await deployFeeCalculator(initParams, {
      commonPrice: domainPrice,
    })
    const { address } = await deployRegistry(initParams, {
      nameCheckerAddress,
      feeCalculatorAddress,
      merkleVerifierAddress: null,
    })
    registryAddress = address
  }

  // Create registry contract instance
  const { abi } = await getDeploymentData('azns_registry')
  const contract = new ContractPromise(api, abi, registryAddress)

  // Set record size limit
  const recordsSizeLimit = new BN(parseInt(process.env.RECORDS_SIZE_LIMIT ?? '8000'))
  await contractTx(api, account, contract, 'set_records_size_limit', {}, [recordsSizeLimit])

  // Track gas usage
  const { balance: balanceStart } = await getBalance(api, account.address, 5)

  // Parse options & print info
  const DOMAIN_COUNT = parseInt(process.env.DOMAIN_COUNT ?? '1000')
  const USE_RANDOM_ACCOUNTS = (process.env.USE_RANDOM_ACCOUNTS || '').toLowerCase() === 'true'
  const META_COUNT = parseInt(process.env.RECORDS_ROW_COUNT ?? '0')
  const META_SIZE = parseInt(process.env.RECORDS_ITEM_SIZE ?? '32')
  console.log(
    `\nRegistering ${DOMAIN_COUNT} domain(s) ${
      USE_RANDOM_ACCOUNTS
        ? `for ${DOMAIN_COUNT} randomly generated account(s)`
        : `for ${account.address}`
    } ${
      META_COUNT
        ? `with ${META_COUNT} metadata record(s) à ${2 * META_SIZE} characters (~${
            2 * META_SIZE * META_COUNT
          } bytes)`
        : 'without metadata records'
    } …`,
  )

  // Progress bar
  let bar = new cliProgress.SingleBar({}, cliProgress.Presets.shades_classic)
  bar.start(DOMAIN_COUNT, 0)

  // Transactions
  for (let i = 0; i < DOMAIN_COUNT; i++) {
    try {
      const uuid = faker.datatype.uuid()
      const domainName = uuid.substring(0, 32)
      let domainAccount = account

      // Generate & fund random account
      if (USE_RANDOM_ACCOUNTS) {
        domainAccount = keyring.addFromUri('//' + domainName)
        const amount = domainPrice.add(toBNWithDecimals(1))
        await transferBalance(api, account, domainAccount.address, amount)
      }

      // Register domain
      await registerDomain(api, domainAccount, contract, domainPrice, domainName)

      // Add metadata records to domain
      await setDomainSampleMetadataRecords(
        api,
        domainAccount,
        contract,
        domainName,
        META_COUNT,
        META_SIZE,
      )

      // Refund rest from random account
      if (USE_RANDOM_ACCOUNTS) {
        await transferFullBalance(api, domainAccount, account.address)
      }
    } catch (e) {
      console.error('Error, aborting:', e)
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
const registerDomain = async (api, account, contract, domainPrice, domainName) => {
  try {
    await contractTx(api, account, contract, 'register', { value: domainPrice }, [
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
const setDomainSampleMetadataRecords = async (
  api,
  account,
  contract,
  domainName,
  rowCount,
  itemSize,
) => {
  if (!rowCount || !itemSize) return

  try {
    const sampleMetadata = new Array(rowCount)
      .fill(null)
      .map(() => [faker.datatype.string(itemSize), faker.datatype.string(itemSize)])
    await contractTx(api, account, contract, 'updateRecords', {}, [
      domainName,
      sampleMetadata,
      true,
    ])
  } catch (e) {
    console.error(`Error while adding metadata records to '${domainName}.azero':`, e)
    throw new Error()
  }
}

main()
  .catch((error) => {
    console.error(error)
    process.exit(1)
  })
  .finally(() => process.exit(0))

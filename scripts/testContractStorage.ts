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
 * Script that tests merkle tree verification off- and on-chain.
 *
 * Example #1: `DOMAIN_COUNT=1000 pnpm ts-node scripts/testContractStorage.ts`
 *   - Tests limitations of reverse-mappings by creating 1000 domains for the same account.
 *   - Fails after 441 domains on Aleph Zero testnet due to storage limitations, see #75.
 *
 * Example #2: `DOMAIN_COUNT=1000 USE_RANDOM_ACCOUNTS=true pnpm ts-node scripts/testContractStorage.ts`
 *   - Should run through as it creates & funds a new random account for each domain.
 *
 * Example #3: `DOMAIN_COUNT=1 METADATA_SIZE_LIMIT=8000 METADATA_ROW_COUNT=120 METADATA_ITEM_SIZE=32 pnpm ts-node scripts/testContractStorage.ts`
 *   - Tests metadata size limits by creating a domain with 120 metadata rows with 64 characters each (32 key & 32 value).
 *   - Fails with lower `METADATA_SIZE_LIMIT` or higher `METADATA_ROW_COUNT` or `METADATA_ITEM_SIZE` (2×32×120 ⪅ 8000 bytes).
 */
const main = async () => {
  const chain = getSubstrateChain(chainId)
  if (!chain) throw new Error(`Chain '${chainId}' not found`)
  const accountUri = process.env.ACCOUNT_URI || '//Alice'
  const initParams = await initPolkadotJs(chain, accountUri)
  const { api, decimals, account, keyring } = initParams

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

  // Parse options & print info
  const DOMAIN_COUNT = parseInt(process.env.DOMAIN_COUNT ?? '1000')
  const USE_RANDOM_ACCOUNTS = (process.env.USE_RANDOM_ACCOUNTS || '').toLowerCase() === 'true'
  const META_COUNT = parseInt(process.env.METADATA_ROW_COUNT ?? '0')
  const META_SIZE = parseInt(process.env.METADATA_ITEM_SIZE ?? '32')
  console.log(
    `\nRegistering ${DOMAIN_COUNT} domain(s) ${
      USE_RANDOM_ACCOUNTS
        ? `for ${DOMAIN_COUNT} randomly generated account(s)`
        : `for ${account.address}`
    } ${
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
      const domainName = faker.datatype.uuid()
      let domainAccount = account

      // Generate & fund random account
      if (USE_RANDOM_ACCOUNTS) {
        domainAccount = keyring.addFromUri('//' + domainName)
        const one = new BN(1).mul(new BN(10 ** decimals))
        await transferBalance(api, account, domainAccount.address, one)
      }

      // Register domain
      await registerDomain(api, domainAccount, contract, domainName)

      // Add metadata to domain
      await setDomainSampleMetadata(api, domainAccount, contract, domainName, META_COUNT, META_SIZE)

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
    await contractTx(api, account, contract, 'updateRecords', {}, [
      domainName,
      sampleMetadata,
      true,
    ])
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

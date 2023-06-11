import { checkAddress } from '@polkadot/util-crypto'
import { existsSync } from 'fs'
import { open } from 'fs/promises'
import path from 'path'
import { InitParams } from 'scripts/utils/initPolkadotJs'

/**
 * Fetch whitelist addresses from .txt file
 * @param relativeFilePath Relative file path to whitelist file
 */
export const getWhitelistAddresses = async ({ prefix }: InitParams, relativeFilePath?: string) => {
  const whitelistFilePath = path.join(path.resolve(), relativeFilePath)
  if (!existsSync(whitelistFilePath)) {
    throw new Error(`Whitelist file not found at '${whitelistFilePath}'. Aborting.`)
  }

  const addresses = []
  const whitelistFile = await open(whitelistFilePath)
  for await (const address of whitelistFile.readLines()) {
    if (!address?.length) continue

    const isValid = checkAddress(address, prefix)[0]
    if (!isValid) {
      throw new Error(`Corrupt address found in whitelist file. Aborting.`)
    }

    addresses.push(address)
  }

  return addresses
}

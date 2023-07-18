import { checkAddress } from '@polkadot/util-crypto'
import { existsSync } from 'fs'
import { open } from 'fs/promises'
import path from 'path'
import { hashAccountId } from '../utils/hashAccountId'
import { InitParams } from '../utils/initPolkadotJs'

/**
 * Fetch whitelist addresses from .txt file
 * @param relativeFilePath Relative file path to whitelist file
 */
export const getWhitelistAddresses = async (
  { prefix }: InitParams,
  relativeFilePath?: string,
  saveHashed?: boolean,
) => {
  const whitelistFilePath = path.join(path.resolve(), relativeFilePath)
  if (!existsSync(whitelistFilePath)) {
    throw new Error(`Whitelist file not found at '${whitelistFilePath}'. Aborting.`)
  }

  const addresses = []
  let line = 0
  const whitelistFile = await open(whitelistFilePath)
  for await (const address of whitelistFile.readLines()) {
    line++

    const _address = (address || '').trim()
    if (!_address?.length) continue

    const isValid = checkAddress(_address, prefix)[0]
    if (!isValid) {
      throw new Error(
        `Corrupt address '${_address}' found in whitelist file on line ${line}. Aborting.`,
      )
    }

    if (addresses.includes(_address)) {
      throw new Error(
        `Duplicate address '${_address}' found in whitelist file on line ${line}. Aborting.`,
      )
    }

    addresses.push(_address)
  }

  // Saves a hashed version of the whitelist addresses to a new file
  if (saveHashed) {
    try {
      const hashedAddresses = addresses.map((address) => hashAccountId(address))
      const relativeHashedFilePath = relativeFilePath.replace(/(\.[^.]+)$/, '.hashed$1')
      const whitelistHashedFilePath = path.join(path.resolve(), relativeHashedFilePath)
      const hashedWhitelistFile = await open(whitelistHashedFilePath, 'w')
      await hashedWhitelistFile.write(hashedAddresses.join('\n'))
      await hashedWhitelistFile.close()
    } catch (e) {
      console.error(e)
      throw new Error(`Error while saving hashed version of whitelist file. Aborting.`)
    }
  }

  return addresses
}

import { checkAddress } from '@polkadot/util-crypto'
import { parse } from 'csv'
import { createReadStream, existsSync } from 'fs'
import path from 'path'
import { InitParams } from 'scripts/utils/initPolkadotJs'
import { sanitizeDomainName } from '../utils/sanitizeDomainName'

type ReservedName = [string, string | null]

/**
 * Fetch reserved names & addresses from .csv file
 * @param relativeFilePath Relative file path to reserved-names  file
 */
export const getReservedNames = async ({ prefix }: InitParams, relativeFilePath?: string) => {
  const csvFilePath = path.join(path.resolve(), relativeFilePath)
  if (!csvFilePath.endsWith('.csv')) {
    throw new Error(`Reserved-names file must be a .csv file. Aborting.`)
  }
  if (!existsSync(csvFilePath)) {
    throw new Error(`Reserved-names file not found at '${csvFilePath}'. Aborting.`)
  }

  const reservedNames: ReservedName[] = []

  const stream = createReadStream(csvFilePath)
  const parser = parse({ delimiter: ';', from_line: 2 })
  const rows = stream.pipe(parser)

  for await (const [name, address] of rows) {
    // Validate domain name
    const _name = sanitizeDomainName(name, {
      lowercase: true,
      trim: true,
      removeDots: true,
      removeOuterNonAlphanumeric: true,
    })
    if (_name !== name) {
      throw new Error(`Invalid name '${name}'. Aborting.`)
    }

    // Validate address
    if (address && !checkAddress(address, prefix)[0]) {
      throw new Error(`Invalid address '${address}' for name '${name}'. Aborting.`)
    }
    const _address = address ? address : null

    const reservedName: ReservedName = [_name, _address]
    reservedNames.push(reservedName)
  }

  if (reservedNames.length === 0) {
    throw new Error(`No reserved names found in '${csvFilePath}'. Aborting.`)
  }

  return reservedNames
}

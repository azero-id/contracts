import { checkAddress } from '@polkadot/util-crypto'
import { InitParams } from '../utils/initPolkadotJs'
import { sanitizeDomainName } from '../utils/sanitizeDomainName'
import { Reservation } from './Reservation.interface'

/**
 * Validates given reserved names & addresses. Throws error if invalid.
 */
export const validateReservations = async ({ prefix }: InitParams, reservations: Reservation[]) => {
  const validatedReservations: Reservation[] = []

  for await (const [name, address] of reservations) {
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

    // Check for duplicates
    if (validatedReservations.find(([n]) => n === _name)) {
      throw new Error(`Duplicate reservation for name '${_name}'. Aborting.`)
    }

    // Add to validated names and addresses
    validatedReservations.push([_name, _address])
  }

  if (validatedReservations.length === 0) {
    throw new Error(`No valid reserved names found. Aborting.`)
  }

  return validatedReservations
}

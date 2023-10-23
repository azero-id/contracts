import { Keyring } from '@polkadot/api'
import { u8aToHex } from '@polkadot/util'
import { sha256 } from 'js-sha256'

/**
 * Parses & Hashes the accountId
 * @param accountId SS58 encoded AccountId
 * @returns SHA256(accountId)
 */
export const hashAccountId = (accountId: string) => {
  const keyring = new Keyring({ type: 'sr25519' })
  const pubkey = u8aToHex(keyring.decodeAddress(accountId), undefined, false)
  return sha256(pubkey)
}

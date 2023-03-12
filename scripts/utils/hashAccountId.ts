import { Keyring } from '@polkadot/api'
import { u8aToHex } from '@polkadot/util'
import cryptojs from 'crypto-js'

/**
 * Parses & Hashes the accountId
 * @param accountId SS58 encoded AccountId
 * @returns SHA256(accountId)
 */
export const hashAccountId = (accountId: string) => {
  const keyring = new Keyring({ type: 'sr25519' })
  const pubkey = u8aToHex(keyring.decodeAddress(accountId))
  const hexkey = cryptojs.enc.Hex.parse(pubkey.slice(2))
  return cryptojs.SHA256(hexkey).toString()
}

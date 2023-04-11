import { ApiPromise, Keyring } from '@polkadot/api'
import { IKeyringPair } from '@polkadot/types/types/interfaces'
import { SubstrateChain, getBalance, initPolkadotJs as initApi } from '@scio-labs/use-inkathon'

/**
 * Initialize Polkadot.js API with given RPC & account from given URI.
 */
export type InitParams = {
  api: ApiPromise
  keyring: Keyring
  account: IKeyringPair
  decimals: number
}
export const initPolkadotJs = async (chain: SubstrateChain, uri: string): Promise<InitParams> => {
  // Initialize api
  const { api } = await initApi(chain)

  // Print chain info
  const network = (await api.rpc.system.chain())?.toString() || ''
  const version = (await api.rpc.system.version())?.toString() || ''
  console.log(`Initialized API on ${network} (${version})`)

  // Get decimals
  const decimals = api.registry.chainDecimals?.[0] || 12

  // Initialize account & set signer
  const keyring = new Keyring({ type: 'sr25519' })
  const account = keyring.addFromUri(uri)
  const balance = await getBalance(api, account.address, 3)
  console.log(
    `Initialized Account: ${account.address} (${balance.balanceFormatted} ${balance.tokenSymbol})\n`,
  )

  return { api, keyring, account, decimals }
}

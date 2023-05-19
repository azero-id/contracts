import { ContractPromise } from '@polkadot/api-contract'
import { checkAddress } from '@polkadot/util-crypto'
import { contractTx } from '@scio-labs/use-inkathon'
import { ContractDeployments } from '../utils/ContractDeployments.type'
import { getDeploymentData } from '../utils/getDeploymentData'
import { InitParams } from '../utils/initPolkadotJs'

/**
 * Updates the admin of a given contract (that implements `set_admin`).
 * Caller must be the current admin.
 */
export const setContractAdmin = async (
  { api, account, prefix }: InitParams,
  contractName: string,
  contractAddress: string,
  newAdmin: string,
) => {
  const isValid = checkAddress(newAdmin, prefix)[0]
  if (!isValid) throw new Error(`Invalid admin address provided: ${newAdmin}. Aborting.`)

  const { abi } = await getDeploymentData(contractName)
  const contract = new ContractPromise(api, abi, contractAddress)
  try {
    await contractTx(api, account, contract, 'set_admin', {}, [newAdmin])
    console.log(`Successfully updated contract admin '${contractName}' to: ${newAdmin}`)
  } catch (error) {
    throw new Error('Error while adding registry to router.')
  }
}

/**
 * Helper function to set the admin of all contracts.
 */
export const setContractAdmins = async (
  initParams: InitParams,
  deployments: ContractDeployments,
) => {
  console.log()
  for (const [name, contract] of Object.entries(deployments)) {
    if (!contract) {
      console.log(`Skipping updating contract admin of '${name}' (no deployment found).`)
      continue
    }
    await setContractAdmin(initParams, name, contract.address, process.env.ADMIN)
  }
}

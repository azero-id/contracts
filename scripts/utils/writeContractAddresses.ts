import { writeFile } from 'fs/promises'
import path from 'path'
import { ContractDeployments } from './ContractDeployments.type'

/**
 * Writes each given contract address & blockNumber to a `{baseDir}/{contract}/{network}.ts` file.
 * NOTE: Base directory can be configured via the `DIR` environment variable
 */
export const writeContractAddresses = async (
  networkId: string,
  contractDeployments: ContractDeployments,
) => {
  const baseDir = process.env.DIR || './deployments'

  console.log()
  for (const [contractName, deployment] of Object.entries(contractDeployments)) {
    const relativePath = path.join(baseDir, contractName, `${networkId}.ts`)
    const absolutePath = path.join(path.resolve(), relativePath)
    let fileContents = ''
    if (deployment?.address) {
      fileContents += `export const address = '${deployment.address}'\n`
    }
    if (deployment?.blockNumber) {
      fileContents += `export const blockNumber = ${deployment.blockNumber}\n`
    }
    await writeFile(absolutePath, fileContents)
    console.log(`Exported deployment info to file: ${relativePath}`)
  }
}

import { readFile } from 'fs/promises'
import path from 'path'

/**
 * Reads the contract deployment files (wasm & abi) from the `basePath` directory.
 */
export const getDeploymentData = async (contractName: string, basePath = './deployments') => {
  const contractPath = path.join(path.resolve(), basePath, contractName)
  let wasm, abi
  try {
    wasm = await readFile(path.join(contractPath, `${contractName}.wasm`))
    abi = JSON.parse(await readFile(path.join(contractPath, 'metadata.json'), 'utf-8'))
  } catch (e) {
    console.error(e)
    throw new Error("Couldn't find contract deployment files. Did you build it via `pnpm build`?")
  }

  return {
    contractPath,
    wasm,
    abi,
  }
}

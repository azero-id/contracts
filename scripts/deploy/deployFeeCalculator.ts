import { BN } from '@polkadot/util'
import { deployContract } from '@scio-labs/use-inkathon'
import { DeployFn } from '../utils/DeployFn.type'
import { getDeploymentData } from '../utils/getDeploymentData'

/**
 * Deploys the `azns_fee_calculator` contract
 */
export type FeeCalculatorArgs = {
  admin: string
  maxRegistrationDuration: number
  commonPrice: BN
  pricePoints: [number, BN][] // [length, price][]
}
export const deployFeeCalculator: DeployFn<FeeCalculatorArgs> = async (
  { api, account, decimals },
  customArgs,
) => {
  const veryHighFee = new BN(1_000_000).mul(new BN(10 ** decimals))
  const args = Object.assign(
    {
      admin: account.address,
      maxRegistrationDuration: 3,
      commonPrice: new BN(6).mul(new BN(10 ** decimals)),
      pricePoints: [
        [1, veryHighFee],
        [2, veryHighFee],
        [3, veryHighFee],
        [4, veryHighFee],
      ],
    } as FeeCalculatorArgs,
    customArgs,
  )
  const { abi, wasm } = await getDeploymentData('azns_fee_calculator')

  return await deployContract(api, account, abi, wasm, 'new', [
    args.admin,
    args.maxRegistrationDuration,
    args.commonPrice,
    args.pricePoints,
  ])
}

import { parse } from 'csv'
import { createReadStream, existsSync } from 'fs'
import path from 'path'
import { InitParams } from '../utils/initPolkadotJs'
import { validateReservations } from './validateReservations'

/**
 * Fetch reserved names & addresses from .csv file given via `RESERVATIONS` env variable.
 */
export const getReservationsFromCSV = async (initParams: InitParams) => {
  const relativeFilePath = process.env.RESERVATIONS
  if (!relativeFilePath) {
    throw new Error(`No reservations file path provided. Aborting.`)
  }

  const csvFilePath = path.join(path.resolve(), relativeFilePath)
  if (!csvFilePath.endsWith('.csv')) {
    throw new Error(`Reserved-names file must be a .csv file. Aborting.`)
  }
  if (!existsSync(csvFilePath)) {
    throw new Error(`Reserved-names file not found at '${csvFilePath}'. Aborting.`)
  }

  const stream = createReadStream(csvFilePath)
  const parser = parse({ delimiter: ';', from_line: 2 })
  const rows = stream.pipe(parser)

  const reservations = await validateReservations(initParams, rows as any)
  const reservationsWithAddress = reservations.filter(([, address]) => !!address).length
  console.log(
    `Loaded ${reservations.length} reserved names (${reservationsWithAddress} with address).`,
  )

  return reservations
}

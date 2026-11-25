import { withTx } from './tx'
import { enqueue } from '@sample/jobs'

export function enqueueInsideBuiltins(rows: { id: string }[]) {
  const sendRow = (row: { id: string }) => enqueue({ id: row.id })
  return withTx(async query => {
    const direct = rows.map(r => enqueue({ id: r.id }))
    const chained = rows.filter(r => r.id.length > 0).map(r => enqueue({ id: r.id }))
    const refCallback = rows.map(enqueue)
    const localHelper = rows.map(sendRow)
    const fromCallback = Array.from(rows, r => enqueue({ id: r.id }))
    const globalFrom = globalThis.Array.from(rows, r => enqueue({ id: r.id }))
    await Promise.resolve().then(() => enqueue({ id: 'then' }))
    await query(`SELECT 1`)
    return { direct, chained, refCallback, localHelper, fromCallback, globalFrom }
  })
}

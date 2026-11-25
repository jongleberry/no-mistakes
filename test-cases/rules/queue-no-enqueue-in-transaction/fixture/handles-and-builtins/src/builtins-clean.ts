import { withTx } from './tx'
import { formatId } from '@sample/utils'

function extractId(row: { id: string }): string {
  return row.id.trim().toLowerCase()
}

export class ItemRunner {
  items: string[] = []

  run(rows: { id: string; tags?: string[] }[], sep: string, extra?: { id: string }[]) {
    return withTx(async query => {
      const seen = new Set<string>()
      const byId = new Map<string, string>()
      const ids = (extra ?? rows)
        .filter(Boolean)
        .map(extractId)
        .map(formatId)
        .filter(id => id.length > 0)
      for (const id of ids) {
        seen.add(id)
        byId.set(id, id.replace(/\s+/g, sep).replaceAll('_', '-'))
      }
      const keys = Object.keys(Object.fromEntries(byId.entries()))
      const joined = Array.from(seen).join(',')
      const payload = JSON.stringify({ keys, joined, ok: Array.isArray(ids) }, null, 2)
      const parsed = JSON.parse(payload)
      const count = Number.isFinite(ids.length) ? Math.min(10, ids.length) : 0
      const absValues = [1, -2].map(Math.abs)
      const trimmed = `prefix-${joined}`.trim()
      const raw = String.raw`SELECT ${String.fromCharCode(49)}`
      const stamp = new Date().toISOString()
      const ownItems = this.items.map(item => item.trim())
      const awaitedIds = (await query<{ id: string }>(`SELECT id FROM items`)).map(r => r.id)
      const nested = await Promise.all(
        awaitedIds.map(id => query<{ id: string }>`SELECT ${id}`),
      )
      await Promise.all(awaitedIds.map(query))
      const tried = await Promise.try(() => count)
      return { count, parsed, raw, stamp, nested, absValues, trimmed, ownItems, tried, hasFirst: seen.has(ids[0] ?? '') }
    })
  }
}

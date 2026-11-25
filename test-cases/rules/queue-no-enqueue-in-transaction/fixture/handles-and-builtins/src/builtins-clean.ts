import { withTx } from './tx'

function extractId(row: { id: string }): string {
  return row.id.trim().toLowerCase()
}

export function cleanBuiltinsAndHandles(
  rows: { id: string; tags?: string[] }[],
  sep: string,
  extra?: { id: string }[],
) {
  return withTx(async query => {
    const seen = new Set<string>()
    const byId = new Map<string, string>()
    const ids = (extra ?? rows)
      .filter(Boolean)
      .map(extractId)
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
    const raw = String.raw`SELECT ${String.fromCharCode(49)}`
    const stamp = new Date().toISOString()
    const fetched = await query<{ id: string }>(`SELECT id FROM items`)
    const nested = await Promise.all(
      fetched.map(r => query<{ id: string }>`SELECT ${r.id}`),
    )
    return { count, parsed, raw, stamp, nested, hasFirst: seen.has(ids[0] ?? '') }
  })
}

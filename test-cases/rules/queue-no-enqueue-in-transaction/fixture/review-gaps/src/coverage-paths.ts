import { withTx } from './tx'

function getTextA(): string {
  return 'a'
}

function getTextB(): string {
  return 'b'
}

export function edgeCoverage(flag: boolean, fnFactory: () => () => void) {
  let rep: unknown = 'x'
  rep = 'y'
  return withTx(async () => {
    const d0: number[] = []
    const d1 = d0
    const d2 = d1
    const d3 = d2
    const d4 = d3
    const d5 = d4
    const d6 = d5
    d6.push(1)
    ;(flag ? getTextA : getTextB)().trim()
    fnFactory()()
    return 'a'.replace('a', rep as string)
  })
}

import { STOCK_NAMES, type Stock } from './entities'

export type Cost = Partial<Record<Stock, number>>

export class Inventory {
  readonly stock: Record<Stock, number> = { steel: 0, plastic: 0, mixed: 0, motor: 0, compressor: 0, battery: 0, chip: 0 }
  /** Debug: build without paying. */
  free = false

  add(s: Stock, n = 1) {
    this.stock[s] += n
  }

  canAfford(cost: Cost) {
    return this.free || Object.entries(cost).every(([k, n]) => this.stock[k as Stock] >= (n ?? 0))
  }

  missing(cost: Cost) {
    return Object.entries(cost)
      .filter(([k, n]) => this.stock[k as Stock] < (n ?? 0))
      .map(([k, n]) => `${n} ${STOCK_NAMES[k as Stock]}`)
  }

  spend(cost: Cost) {
    if (this.free) return
    for (const [k, n] of Object.entries(cost)) this.stock[k as Stock] -= n ?? 0
  }

  refund(cost: Cost) {
    if (this.free) return
    for (const [k, n] of Object.entries(cost)) this.stock[k as Stock] += n ?? 0
  }
}

export function describeCost(cost: Cost) {
  const parts = Object.entries(cost).map(([k, n]) => `${n} ${STOCK_NAMES[k as Stock]}`)
  return parts.length ? parts.join(' + ') : 'free'
}

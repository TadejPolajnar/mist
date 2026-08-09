import { store } from 'mist'

const TICKS = [1012, 991, 1004, 1018, 987, 1002, 1009, 994, 1021, 983, 1006, 998]

export const market = store(
  {
    tick: 0,
    history: [1934500, 1948200, 1952800, 1961400, 1955300, 1976250],
    positions: [
      { id: 'moutai', name: '贵州茅台', emoji: '🍶', sector: '消费', qty: 2, cost: 158000, price: 161250, prev: 159800 },
      { id: 'catl', name: '宁德时代', emoji: '🔋', sector: '新能源', qty: 20, cost: 21050, price: 19860, prev: 20120 },
      { id: 'byd', name: '比亚迪', emoji: '🚗', sector: '新能源', qty: 15, cost: 24800, price: 26430, prev: 26010 },
      { id: 'tencent', name: '腾讯控股', emoji: '🐧', sector: '科技', qty: 10, cost: 32000, price: 35880, prev: 35400 },
      { id: 'smic', name: '中芯国际', emoji: '🔬', sector: '科技', qty: 40, cost: 5210, price: 4890, prev: 4995 },
      { id: 'yili', name: '伊利股份', emoji: '🥛', sector: '消费', qty: 60, cost: 2860, price: 2705, prev: 2688 },
      { id: 'pingan', name: '中国平安', emoji: '🛡️', sector: '金融', qty: 30, cost: 4520, price: 4780, prev: 4732 },
    ],
  },
  { persist: 'folio.market', version: 1 }
)

export function applyTick() {
  const n = market.value.positions.length
  for (let i = 0; i < n; i++) {
    const f = TICKS[(market.value.tick + i) % TICKS.length]
    market.value.positions[i].prev = market.value.positions[i].price
    market.value.positions[i].price = Math.round(market.value.positions[i].price * f / 1000)
  }
  let total = 0
  for (const p of market.value.positions) {
    total += p.qty * p.price
  }
  market.value.history = market.value.history.concat(total).slice(-12)
  market.value.tick++
}

export function trade(id, delta) {
  const i = market.value.positions.findIndex(p => p.id === id)
  if (i < 0 || market.value.positions[i].qty + delta < 0) {
    return
  }
  if (delta > 0) {
    const qty = market.value.positions[i].qty
    const cost = market.value.positions[i].cost
    const price = market.value.positions[i].price
    market.value.positions[i].cost = Math.round((qty * cost + delta * price) / (qty + delta))
  }
  market.value.positions[i].qty += delta
}

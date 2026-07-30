import { useSyncExternalStore } from 'react'

let state = {
  txs: [
    { id: 1, title: 'Groceries', amount: 82, cat: 'Food', icon: '🛒', ts: 1785150000000 },
    { id: 2, title: 'Metro card', amount: 25, cat: 'Transit', icon: '🚇', ts: 1785236400000 },
    { id: 3, title: 'Coffee', amount: 4, cat: 'Food', icon: '☕', ts: 1785322800000 },
  ],
  nextId: 4,
  budget: 600,
}
const subs = new Set()
const getState = () => state
function setState(patch) {
  state = { ...state, ...patch }
  subs.forEach((f) => f())
}
export function useLedger() {
  return useSyncExternalStore((cb) => (subs.add(cb), () => subs.delete(cb)), getState)
}
export function addTx(title, amount, cat, icon, ts) {
  setState({ txs: [...state.txs, { id: state.nextId, title, amount, cat, icon, ts }], nextId: state.nextId + 1 })
}
export function removeTx(id) {
  setState({ txs: state.txs.filter((t) => t.id !== id) })
}
export const fmtDate = (ts) => { const d = new Date(ts); return d.getMonth() + 1 + '/' + d.getDate() }
export const fmtFull = (ts) => { const d = new Date(ts); return d.getFullYear() + '-' + (d.getMonth() + 1) + '-' + d.getDate() }

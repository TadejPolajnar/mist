import { store } from 'mist'

export const ledger = store({
  txs: [
    { id: 1, title: 'Groceries', amount: 82, cat: 'Food', icon: '🛒', ts: 1785150000000 },
    { id: 2, title: 'Metro card', amount: 25, cat: 'Transit', icon: '🚇', ts: 1785236400000 },
    { id: 3, title: 'Coffee', amount: 4, cat: 'Food', icon: '☕', ts: 1785322800000 },
  ],
  nextId: 4,
  budget: 600,
})

export function addTx(title, amount, cat, icon, ts) {
  ledger.value.txs.push({ id: ledger.value.nextId, title, amount, cat, icon, ts })
  ledger.value.nextId++
}

export function setBudget(v) {
  ledger.value.budget = v
}

export function removeTx(id) {
  ledger.value.txs = ledger.value.txs.filter(t => t.id !== id)
}

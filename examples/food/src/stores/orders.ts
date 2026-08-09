import { store } from 'mist'

export const orders = store(
  { list: [], nextOrder: 1 },
  {
    persist: 'food.orders',
    version: 2,
    migrate(old) {
      return {
        list: (old.list || []).map(o => ({ ...o, discount: 0 })),
        nextOrder: old.nextOrder || 1,
      }
    },
  }
)

export function appendOrder(lines, total, discount, pickup, note) {
  orders.value.list.push({
    order: orders.value.nextOrder,
    lines: lines.map(l => ({ ...l })),
    total,
    discount,
    pickup,
    note,
    status: 'making',
  })
  orders.value.nextOrder++
}

export function markDone(order) {
  const i = orders.value.list.findIndex(o => o.order === order)
  if (i >= 0) {
    orders.value.list[i].status = 'done'
  }
}

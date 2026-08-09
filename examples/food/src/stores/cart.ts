import { store } from 'mist'

export const cart = store({ lines: [], nextLine: 1 }, { persist: 'food.cart', version: 1 })

export function addLine(itemId, name, emoji, unit, qty, choices) {
  cart.value.lines.push({ line: cart.value.nextLine, itemId, name, emoji, unit, qty, choices })
  cart.value.nextLine++
}

export function setQty(line, qty) {
  const i = cart.value.lines.findIndex(l => l.line === line)
  if (i >= 0 && qty >= 1) {
    cart.value.lines[i].qty = qty
  }
}

export function removeLine(line) {
  cart.value.lines = cart.value.lines.filter(l => l.line !== line)
}

export function clearCart() {
  cart.value.lines = []
  cart.value.nextLine = 1
}

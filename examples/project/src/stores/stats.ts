import { store } from 'mist'

export const stats = store({ taps: 0, lastAction: 'none' })

export function track(action) {
  stats.value.taps++
  stats.value.lastAction = action
}

import { store } from 'mist'

export const prefs = store({ wipLimit: 3 }, { persist: 'kanban.prefs', version: 1 })

export function setWipLimit(n) {
  if (n >= 1 && n <= 9) {
    prefs.value.wipLimit = n
  }
}

import { store } from 'mist'

export const prefs = store({ thresholdBp: 150, sort: 'value' }, { persist: 'folio.prefs', version: 1 })

export function setThresholdBp(bp) {
  if (bp >= 50 && bp <= 500) {
    prefs.value.thresholdBp = bp
  }
}

export function setSort(mode) {
  prefs.value.sort = mode
}

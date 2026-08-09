import { store } from 'mist'

export const ui = store({ cat: '' })

export function setCat(id) {
  ui.value.cat = id
}

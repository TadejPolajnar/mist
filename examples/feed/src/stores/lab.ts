import { store } from 'mist'

export const lab = store({ fullRender: false, pageSize: 50 })

export function setFullRender(on) {
  lab.value.fullRender = on
}

export function setPageSize(n) {
  if (n >= 10 && n <= 200) {
    lab.value.pageSize = n
  }
}

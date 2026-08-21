import { store } from 'mist'

const dict = {
  en: {
    'nav.title': 'Mist i18n',
    'greet.title': 'Hello',
    'greet.body': 'The whole page re-renders when the locale store changes.',
    'action.switch': '切换到中文',
  },
  zh: {
    'nav.title': '雾语 i18n',
    'greet.title': '你好',
    'greet.body': '语言 store 一变，整页字符串立即重新渲染。',
    'action.switch': 'Switch to English',
  },
}

export const locale = store('en', { persist: 'locale' })

export function t(key: string): string {
  return dict[locale.value][key] || key
}

export function setLocale(next: string) {
  locale.value = next
}

import { store } from 'mist'

const FLOW = ['backlog', 'todo', 'doing', 'review', 'done']

export const board = store(
  {
    nextId: 12,
    cards: [
      { id: 1, title: '登录页视觉稿', col: 'todo', order: 1, assignee: 'yan', tag: '设计' },
      { id: 2, title: '接入微信支付', col: 'todo', order: 2, assignee: 'bo', tag: '开发' },
      { id: 3, title: '首页骨架屏', col: 'todo', order: 3, assignee: '', tag: '开发' },
      { id: 4, title: '订单列表分页', col: 'doing', order: 1, assignee: 'bo', tag: '开发' },
      { id: 5, title: '新人引导文案', col: 'doing', order: 2, assignee: 'mei', tag: '文案' },
      { id: 6, title: '购物车缓存策略', col: 'review', order: 1, assignee: 'bo', tag: '开发' },
      { id: 7, title: '品牌色规范', col: 'review', order: 2, assignee: 'yan', tag: '设计' },
      { id: 8, title: '启动页合规检查', col: 'done', order: 1, assignee: 'mei', tag: '运营' },
      { id: 9, title: '埋点方案评审', col: 'done', order: 2, assignee: 'yan', tag: '开发' },
      { id: 10, title: '会员体系调研', col: 'backlog', order: 1, assignee: '', tag: '运营' },
      { id: 11, title: '深色模式支持', col: 'backlog', order: 2, assignee: '', tag: '设计' },
    ],
  },
  { persist: 'kanban.board', version: 1 }
)

export function moveVert(id, dir) {
  const i = board.value.cards.findIndex(c => c.id === id)
  if (i < 0) {
    return
  }
  const col = board.value.cards[i].col
  const ord = board.value.cards[i].order
  let j = -1
  for (let k = 0; k < board.value.cards.length; k++) {
    const c = board.value.cards[k]
    if (k === i || c.col !== col) {
      continue
    }
    if (dir > 0 && c.order > ord && (j < 0 || c.order < board.value.cards[j].order)) {
      j = k
    }
    if (dir < 0 && c.order < ord && (j < 0 || c.order > board.value.cards[j].order)) {
      j = k
    }
  }
  if (j < 0) {
    return
  }
  const other = board.value.cards[j].order
  board.value.cards[j].order = ord
  board.value.cards[i].order = other
}

export function moveCol(id, dir) {
  const i = board.value.cards.findIndex(c => c.id === id)
  if (i < 0) {
    return
  }
  const ci = FLOW.indexOf(board.value.cards[i].col)
  const ni = ci + dir
  if (ni < 1 || ni >= FLOW.length) {
    return
  }
  let max = 0
  for (const c of board.value.cards) {
    if (c.col === FLOW[ni] && c.order > max) {
      max = c.order
    }
  }
  board.value.cards[i].col = FLOW[ni]
  board.value.cards[i].order = max + 1
}

export function addCard(title, tag) {
  let max = 0
  for (const c of board.value.cards) {
    if (c.col === 'backlog' && c.order > max) {
      max = c.order
    }
  }
  board.value.cards.push({ id: board.value.nextId, title, col: 'backlog', order: max + 1, assignee: '', tag })
  board.value.nextId++
}

export function removeCard(id) {
  board.value.cards = board.value.cards.filter(c => c.id !== id)
}

export function assign(id, member) {
  const i = board.value.cards.findIndex(c => c.id === id)
  if (i >= 0) {
    board.value.cards[i].assignee = member
  }
}

import { store } from 'mist'

const AVATARS = ['🦊', '🐼', '🐱', '🐯', '🐸', '🦁', '🐰', '🐨']

export const team = store(
  {
    nextMember: 4,
    members: [
      { id: 'yan', name: '燕妮', avatar: '🦊' },
      { id: 'bo', name: '阿波', avatar: '🐼' },
      { id: 'mei', name: '小美', avatar: '🐱' },
    ],
  },
  { persist: 'kanban.team', version: 1 }
)

export function addMember(name) {
  if (!name) {
    return
  }
  const avatar = AVATARS[team.value.nextMember % AVATARS.length]
  team.value.members.push({ id: 'm' + team.value.nextMember, name, avatar })
  team.value.nextMember++
}

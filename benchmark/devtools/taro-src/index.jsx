// Equivalent bench page for Taro 3 + React. Drop into a scaffolded Taro app as
// src/pages/index/index.jsx (see ../README.md for scaffold steps).
import { useState, useMemo } from 'react'
import { View, Text, Button } from '@tarojs/components'

const initial = Array.from({ length: 1000 }, (_, i) => ({
  id: i + 1,
  title: 'Task number ' + (i + 1),
  done: i % 3 === 0,
}))

export default function Index() {
  const [todos, setTodos] = useState(initial)
  const [filter, setFilter] = useState('all')

  const visible = useMemo(
    () => (filter === 'all' ? todos : todos.filter((t) => !t.done)),
    [todos, filter]
  )

  const toggle = (id) => {
    setTodos((ts) => ts.map((t) => (t.id === id ? { ...t, done: !t.done } : t)))
  }

  return (
    <View className="p-2">
      <Button className="bench-filter" onClick={() => setFilter(filter === 'all' ? 'open' : 'all')}>
        Filter: {filter}
      </Button>
      {visible.map((t) => (
        <View className="bench-row" key={t.id} onClick={() => toggle(t.id)}>
          <Text style={t.done ? { textDecoration: 'line-through', color: '#9ca3af' } : {}}>
            {t.title}
          </Text>
        </View>
      ))}
    </View>
  )
}

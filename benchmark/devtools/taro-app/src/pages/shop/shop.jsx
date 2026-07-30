import { useState, useMemo } from 'react'
import { View, Text, Button } from '@tarojs/components'

const products = Array.from({ length: 100 }, (_, i) => ({
  id: i + 1,
  name: 'Product ' + (i + 1),
  price: ((i % 9) + 1) * 10,
  cat: ['a', 'b', 'c', 'd'][i % 4],
}))

export default function Shop() {
  const [category, setCategory] = useState('all')
  const [cart, setCart] = useState([])

  const visible = useMemo(
    () => (category === 'all' ? products : products.filter((p) => p.cat === category)),
    [category]
  )
  const count = useMemo(() => cart.reduce((s, c) => s + c.qty, 0), [cart])
  const total = useMemo(() => cart.reduce((s, c) => s + c.qty * c.price, 0), [cart])

  const add = (id) => {
    setCart((cs) => {
      const i = cs.findIndex((c) => c.id === id)
      if (i >= 0) return cs.map((c, j) => (j === i ? { ...c, qty: c.qty + 1 } : c))
      const p = products.find((p) => p.id === id)
      return [...cs, { id: p.id, name: p.name, price: p.price, qty: 1 }]
    })
  }

  return (
    <View className="p-2">
      <View>
        <Text>Cart: {count} items ¥{total}</Text>
      </View>
      <Button className="bench-filter" onClick={() => setCategory(category === 'all' ? 'b' : 'all')}>
        Category: {category}
      </Button>
      {visible.map((p) => (
        <View className="bench-row" key={p.id} onClick={() => add(p.id)}>
          <Text>{p.name}</Text>
          <Text> ¥{p.price}</Text>
        </View>
      ))}
      <View>
        {cart.map((c) => (
          <View key={c.id}>
            <Text>{c.name} × {c.qty}</Text>
          </View>
        ))}
      </View>
    </View>
  )
}

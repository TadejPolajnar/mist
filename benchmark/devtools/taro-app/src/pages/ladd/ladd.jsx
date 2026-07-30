import Taro from '@tarojs/taro'
import { View, Text, Input } from '@tarojs/components'
import { useState, useEffect } from 'react'
import { addTx } from '../../ledger/store'
import '../../ledger/ledger.css'

const ICONS = { Food: '🍜', Transit: '🚇', Fun: '🎮' }

export default function LAdd() {
  const [title, setTitle] = useState('')
  const [amount, setAmount] = useState('')
  const [cat, setCat] = useState('Food')
  const valid = title.length > 0 && Number(amount) > 0

  // bench hook: expose the same state change the pill tap performs, so the
  // eval harness can drive it identically to mist's p.pick(c)
  useEffect(() => {
    const pages = Taro.getCurrentPages()
    const p = pages[pages.length - 1]
    if (p) p.__setCat = (c) => setCat(c)
  }, [])

  const save = () => {
    if (!valid) return
    addTx(title, Number(amount), cat, ICONS[cat] || '?', Date.now())
    setTitle('')
    setAmount('')
    Taro.switchTab({ url: '/pages/lhome/lhome' })
  }

  return (
    <View className="wrap">
      <View className="card center">
        <Text className="muted">Amount</Text>
        <Input className="input-big" type="digit" value={amount} onInput={(e) => setAmount(e.detail.value)} placeholder="¥0" />
      </View>
      <View className="col">
        <Text className="muted">What was it?</Text>
        <Input className="card input" value={title} onInput={(e) => setTitle(e.detail.value)} placeholder="Lunch, taxi..." />
      </View>
      <View className="col">
        <Text className="muted">Category</Text>
        <View className="pills">
          {['Food', 'Transit', 'Fun'].map((c) => (
            <View key={c} className={(cat === c ? 'pill-on' : 'pill') + ' bench-pill'} onClick={() => setCat(c)}>
              {ICONS[c]} {c}
            </View>
          ))}
        </View>
      </View>
      <View className={'btn ' + (valid ? 'btn-on' : 'btn-off')} onClick={save}>Add expense</View>
    </View>
  )
}

import { View, Text } from '@tarojs/components'
import { useLedger } from '../../ledger/store'
import '../../ledger/ledger.css'

const COLORS = { Food: '#5ea0ff', Transit: '#34d399', Fun: '#c084fc' }

export default function LStats() {
  const { txs } = useLedger()
  const total = txs.reduce((s, t) => s + t.amount, 0)
  const sums = {}
  for (const t of txs) sums[t.cat] = (sums[t.cat] || 0) + t.amount
  const cats = Object.keys(sums)
    .map((c) => ({ cat: c, amount: sums[c], pct: total ? Math.round((sums[c] / total) * 100) : 0, color: COLORS[c] || '#f59e0b' }))
    .sort((a, b) => b.amount - a.amount)

  return (
    <View className="wrap">
      <View className="card row between">
        <View className="col"><Text className="muted">Total spent</Text><Text className="h1">¥{total}</Text></View>
        <View className="col"><Text className="muted">Expenses</Text><Text className="h1">{txs.length}</Text></View>
      </View>
      <Text className="title">By category</Text>
      <View className="card col" style={{ padding: '32rpx' }}>
        {cats.map((c) => (
          <View key={c.cat} className="col">
            <View className="between">
              <Text className="title">{c.cat}</Text>
              <Text className="muted">¥{c.amount} - {c.pct}%</Text>
            </View>
            <View className="bar-bg"><View className="bar" style={{ width: c.pct + '%', background: c.color }} /></View>
          </View>
        ))}
      </View>
    </View>
  )
}

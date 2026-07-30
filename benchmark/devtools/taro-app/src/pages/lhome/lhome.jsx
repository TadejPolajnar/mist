import Taro from '@tarojs/taro'
import { View, Text } from '@tarojs/components'
import { useLedger, fmtDate } from '../../ledger/store'
import '../../ledger/ledger.css'

export default function LHome() {
  const { txs, budget } = useLedger()
  const spent = txs.reduce((s, t) => s + t.amount, 0)
  const pct = Math.min(100, Math.round((spent / budget) * 100))
  const recent = txs.slice().reverse()

  return (
    <View className="wrap">
      <View className="col">
        <Text className="muted">July 2026</Text>
        <Text className="h1">Spending</Text>
      </View>
      <View className="hero">
        <Text className="sub">Spent this month</Text>
        <Text className="big">¥{spent.toLocaleString()}</Text>
        <View className="track"><View className="fill" style={{ width: pct + '%' }} /></View>
        <View className="between">
          <Text className="sub">¥{(budget - spent).toLocaleString()} left</Text>
          <Text className="sub">of ¥{budget.toLocaleString()}</Text>
        </View>
      </View>
      <View className="between">
        <Text className="title">Recent</Text>
        <Text className="muted">{recent.length} items</Text>
      </View>
      {recent.length === 0 && (
        <View className="card center">
          <Text style={{ fontSize: '60rpx' }}>🌱</Text>
          <Text className="muted">No expenses yet</Text>
        </View>
      )}
      <View className="col">
        {recent.map((t) => (
          <View key={t.id} className="card row" onClick={() => Taro.navigateTo({ url: '/pages/ldetail/ldetail?id=' + t.id })}>
            <View className="icon"><Text>{t.icon}</Text></View>
            <View className="grow">
              <Text className="title">{t.title}</Text>
              <Text className="muted">{t.cat} - {fmtDate(t.ts)}</Text>
            </View>
            <Text className="amt">-¥{t.amount}</Text>
          </View>
        ))}
      </View>
    </View>
  )
}

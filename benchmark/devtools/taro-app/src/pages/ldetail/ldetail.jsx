import Taro, { useRouter } from '@tarojs/taro'
import { View, Text } from '@tarojs/components'
import { useLedger, removeTx, fmtFull } from '../../ledger/store'
import '../../ledger/ledger.css'

export default function LDetail() {
  const { params } = useRouter()
  const { txs } = useLedger()
  const tx = txs.find((t) => t.id === Number(params.id)) || { title: '?', amount: 0, cat: '?', icon: '?', ts: 0 }

  const del = () => {
    removeTx(Number(params.id))
    Taro.navigateBack()
  }

  return (
    <View className="wrap">
      <View className="card center">
        <View className="icon big-icon"><Text>{tx.icon}</Text></View>
        <Text className="h1">{tx.title}</Text>
        <Text className="big">-¥{tx.amount}</Text>
        <Text className="muted">{tx.cat} - {fmtFull(tx.ts)}</Text>
      </View>
      <View className="btn card danger" onClick={del}>Delete expense</View>
    </View>
  )
}

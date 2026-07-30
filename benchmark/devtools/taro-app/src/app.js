import { Component } from 'react'

// same perf instrumentation as mist's generated app.js: observer installed at
// module evaluation, entries exposed via getApp().__perf for measure.js
const perfEntries = []
try {
  const perf = wx.getPerformance()
  try {
    perfEntries.push(...perf.getEntries())
  } catch (e) {}
  const obs = perf.createObserver((list) => {
    perfEntries.push(...list.getEntries())
  })
  obs.observe({ entryTypes: ['navigation', 'render', 'script'] })
} catch (e) {}

class App extends Component {
  __perf = perfEntries

  render() {
    return this.props.children
  }
}
export default App

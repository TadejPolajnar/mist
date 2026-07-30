export default {
  pages: [
    'pages/ladd/ladd',
    'pages/index/index',
    'pages/shop/shop',
    'pages/lhome/lhome',
    'pages/lstats/lstats',
    'pages/ldetail/ldetail',
  ],
  window: { navigationBarTitleText: 'Taro bench' },
  tabBar: {
    color: '#6a7282',
    selectedColor: '#5ea0ff',
    backgroundColor: '#0e1526',
    borderStyle: 'black',
    list: [
      { pagePath: 'pages/lhome/lhome', text: 'Home' },
      { pagePath: 'pages/ladd/ladd', text: 'Add' },
      { pagePath: 'pages/lstats/lstats', text: 'Stats' },
    ],
  },
}

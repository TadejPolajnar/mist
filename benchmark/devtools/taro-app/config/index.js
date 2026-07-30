const config = {
  projectName: 'taro-bench',
  date: '2026-7-29',
  designWidth: 750,
  deviceRatio: { 640: 2.34 / 2, 750: 1, 828: 1.81 / 2 },
  sourceRoot: 'src',
  outputRoot: 'dist',
  framework: 'react',
  compiler: 'webpack5',
  plugins: [],
  mini: { postcss: { autoprefixer: { enable: false } } },
};
module.exports = function (merge) {
  return merge({}, config, {});
};

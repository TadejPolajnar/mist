const assert = require('node:assert');

module.exports = async () => {
  const app = bootPage('index');
  await flush();
  assert.equal(app.data()._h0, 'Hello');

  app.page.toggle();
  await flush();
  assert.equal(app.data()._h0, '你好');
  assert.equal(app.data()._h2, 'Switch to English');

  app.page.toggle();
  await flush();
  assert.equal(app.data()._h0, 'Hello');

  app.page.onShow();
  const titles = wx.__calls.filter(c => c.name === 'setNavigationBarTitle');
  assert.ok(titles.length >= 1, 'onShow should retitle the nav bar');
};

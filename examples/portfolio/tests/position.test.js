const assert = require('node:assert');

module.exports = async () => {
  const app = bootPage('pages/position/position', { query: { id: 'catl' } });
  await flush();

  assert.ok(!wx.__calls.some((c) => c.name === 'navigateBack'), 'valid id must not bounce');
  assert.equal(app.data().missing, false);
  assert.equal(app.data().pos.name, '宁德时代');
  assert.equal(app.data().pos.qtyStr, '20 股');

  app.page.buyOne();
  await flush();
  assert.equal(app.data().pos.qtyStr, '21 股');

  for (let i = 0; i < 21; i++) {
    app.page.sellOne();
    await flush();
  }
  assert.equal(app.data().pos.qtyStr, '0 股');
  assert.equal(app.data().pos.canSell, false, 'empty position must disable selling');

  app.page.sellOne();
  await flush();
  assert.equal(app.data().pos.qtyStr, '0 股', 'selling at zero must be a no-op');
};

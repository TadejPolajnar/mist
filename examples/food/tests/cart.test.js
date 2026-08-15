const assert = require('node:assert');

module.exports = async () => {
  const store = load('stores/cart');
  const app = bootPage('cart');
  await flush();

  assert.equal(app.data().itemCount, 0);

  store.addLine('latte', '拿铁', '☕', 18, 1, []);
  await flush();
  assert.equal(app.data().itemCount, 1);
  assert.equal(app.data().payable, 18);
  assert.equal(app.data().gap, 12, 'need 12 more for the -3 discount');

  store.addLine('mocha', '摩卡', '🍫', 20, 1, []);
  await flush();
  assert.equal(app.data().discount, 3, 'over-30 order must get the discount');
  assert.equal(app.data().payable, 35);
  assert.equal(app.data().gap, 0);

  app.page.incLine(1);
  await flush();
  assert.equal(app.data().payable, 53);
  assert.ok(app.lastPatch().size < 500, `qty patch too large: ${app.lastPatch().size} bytes`);

  appHide();
  const persisted = wx.__storage.get('food.cart');
  assert.ok(persisted, 'cart must persist on app hide');
};

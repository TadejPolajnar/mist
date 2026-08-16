const assert = require('node:assert');

module.exports = async () => {
  const store = load('stores/ledger');
  const home = bootPage('index');
  await flush();

  assert.equal(home.data().spent, 111);
  assert.equal(home.data().left, 489);
  assert.equal(home.data().recent[0].title, 'Coffee');

  store.addTx('Lunch', 30, 'Food', '🍜', 1785409200000);
  await flush();
  assert.equal(home.data().spent, 141);
  assert.equal(home.data().recent[0].title, 'Lunch');

  const detail = bootPage('detail', { query: { id: '2' } });
  await flush();
  assert.equal(detail.data().tx.title, 'Metro card');

  store.removeTx(2);
  await flush();
  assert.equal(home.data().spent, 116);

  appHide();
  const persisted = wx.__storage.get('ledger');
  assert.ok(persisted, 'ledger must persist on app hide');
  assert.equal(persisted.data.txs.length, 3, 'persisted state must reflect mutations');
};

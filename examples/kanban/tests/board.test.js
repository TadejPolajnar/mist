const assert = require('node:assert');

module.exports = async () => {
  const board = load('stores/board');
  const prefs = load('stores/prefs');
  const app = bootPage('board');
  await flush();

  const cols = app.data().grouped;
  assert.equal(cols.length, 4);
  assert.equal(cols[1].count, 2, 'doing starts with 2 cards');
  assert.equal(cols[1].over, false);
  assert.equal(cols[0].cards[0].who, '🦊 燕妮', 'cross-store derived must resolve assignees');

  prefs.setWipLimit(1);
  await flush();
  assert.equal(app.data().grouped[1].over, true, 'WIP breach must flag the column');

  board.moveCol(1, 1);
  await flush();
  assert.equal(app.data().grouped[0].count, 2);
  assert.equal(app.data().grouped[1].count, 3);

  appHide();
  assert.ok(wx.__storage.get('kanban.board'), 'board must persist on app hide');
  assert.ok(wx.__storage.get('kanban.prefs'), 'prefs must persist on app hide');
};

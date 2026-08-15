const assert = require('node:assert');

module.exports = async () => {
  const app = bootPage('dashboard');
  await flush();

  const d = app.data();
  assert.equal(d.totalValue, 1976250);
  assert.equal(d.totalCost, 1944600);
  assert.equal(d.headline.pnl, '+316.50');
  assert.equal(d.headline.tone, 'up');
  assert.equal(d.allocation.length, 4);
  assert.equal(d.movers[0].id, 'smic');

  app.page.refresh();
  await flush(10);
  assert.equal(app.data().totalValue, 1983620);
  assert.equal(app.data().spark.length, 7);
  assert.equal(
    app.data().totalPnl,
    app.data().totalValue - app.data().totalCost,
    'derived DAG must stay consistent after a tick'
  );
};

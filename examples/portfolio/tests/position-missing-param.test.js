const assert = require('node:assert');

module.exports = async () => {
  const app = bootPage('pages/position/position');
  await flush();

  assert.ok(
    wx.__calls.some((c) => c.name === 'navigateBack'),
    'missing route param must navigate back'
  );
  assert.equal(app.patches.length, 0, 'guarded page must not render');
  assert.ok(!app.data().pos, 'no position must be derived');
};

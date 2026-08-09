const automator = require('miniprogram-automator');
const path = require('path');

const CLI = '/Applications/wechatwebdevtools.app/Contents/MacOS/cli';
const ROOT = path.resolve(__dirname, '../../examples');

function step(label, promise, ms) {
  return Promise.race([
    promise,
    new Promise((_, rej) => setTimeout(() => rej(new Error(`timeout at: ${label}`)), ms || 30000)),
  ]).then((v) => {
    console.log(`  ${label}: done`);
    return v;
  });
}

function assert(cond, msg) {
  if (!cond) {
    throw new Error(msg);
  }
}

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

async function withApp(name, fn) {
  console.log(`${name}: launching…`);
  const mp = await step('launch', automator.launch({ cliPath: CLI, projectPath: path.join(ROOT, name) }), 120000);
  try {
    await fn(mp);
    console.log(`${name}: OK`);
  } finally {
    await mp.close().catch(() => {});
  }
}

const topData = () => {
  const pages = getCurrentPages();
  const p = pages[pages.length - 1];
  return JSON.parse(JSON.stringify({ route: p.route, data: p.data }));
};

async function main() {
  await withApp('portfolio', async (mp) => {
    const home = await step('currentPage', mp.currentPage());
    assert(home.path === 'pages/dashboard/dashboard', `portfolio launch page wrong: ${home.path}`);
    await step('navigateTo', mp.navigateTo('/pages/position/position?id=catl'));
    await wait(400);
    const before = await step('eval-before', mp.evaluate(topData));
    assert(before.route === 'pages/position/position', `wrong page: ${before.route}`);
    assert(before.data.pos && before.data.pos.name === '宁德时代', `position not loaded: ${JSON.stringify(before.data.pos)}`);
    await step(
      'buyOne',
      mp.evaluate(() => {
        const pages = getCurrentPages();
        pages[pages.length - 1].buyOne();
      })
    );
    await wait(400);
    const after = await step('eval-after', mp.evaluate(topData));
    const qtyBefore = parseInt(before.data.pos.qtyStr, 10);
    const qtyAfter = parseInt(after.data.pos.qtyStr, 10);
    assert(qtyAfter === qtyBefore + 1, `buy did not apply: ${before.data.pos.qtyStr} -> ${after.data.pos.qtyStr}`);
    const pnlBefore = parseFloat(before.data.pos.pnlStr);
    const pnlAfter = parseFloat(after.data.pos.pnlStr);
    assert(
      Math.abs(pnlAfter - pnlBefore) <= 0.5,
      `buy fabricated P&L beyond rounding: ${before.data.pos.pnlStr} -> ${after.data.pos.pnlStr}`
    );
  });

  await withApp('food', async (mp) => {
    const home = await step('currentPage', mp.currentPage());
    assert(home.path === 'pages/index/index', `food launch page wrong: ${home.path}`);
    await step(
      'seed order',
      mp.evaluate(() => {
        const o = require('stores/orders.js');
        o.appendOrder(
          [{ line: 999, itemId: 'grape', name: '多肉葡萄', emoji: '🍇', unit: 19, qty: 2, choices: '' }],
          35,
          3,
          '12:30',
          ''
        );
      })
    );
    await step('switchTab', mp.switchTab('/pages/orders/orders'));
    await wait(400);
    const view = await step('eval-orders', mp.evaluate(topData));
    assert(view.route === 'pages/orders/orders', `wrong page: ${view.route}`);
    const latest = view.data.recent[0];
    assert(latest.discount === 3, `order discount missing: ${JSON.stringify(latest)}`);
    const lineSum = latest.lines.reduce((s, l) => s + l.unit * l.qty, 0);
    assert(
      lineSum - latest.discount === latest.total,
      `receipt math wrong: ${lineSum} - ${latest.discount} != ${latest.total}`
    );
  });
}

main().then(
  () => {
    console.log('SMOKE OK');
    process.exit(0);
  },
  (e) => {
    console.error('SMOKE FAILED:', e.message);
    process.exit(1);
  }
);

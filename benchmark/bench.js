// Bridge-traffic benchmark: setData calls and payload bytes per interaction.
//
// The logic-thread ↔ render-thread bridge is the mini-program bottleneck; every
// framework ultimately pays in setData payloads. This harness runs the exact
// runtime mist ships (mist-rt.js) with the exact code shapes mistc emits, against
// two baselines, on a 1000-row filtered list:
//
//   mist          — compiled semantics: path-precise writes, keyed derived diff, batching
//   hand-optimal  — theoretical floor: a human writing perfect setData paths, no derived list
//   naive         — typical quick native code: resend the whole list every change
//
// Run: node benchmark/bench.js

const rt = require('../runtime/mist-rt.js');

const N = 1000;
const TOGGLES = 100;

function makeTodos() {
  return Array.from({ length: N }, (_, i) => ({ id: i + 1, title: `Task number ${i + 1}`, done: i % 3 === 0 }));
}

function makePage(data, derive) {
  const stats = { calls: 0, bytes: 0 };
  const page = {
    data,
    setData(payload) {
      stats.calls++;
      stats.bytes += JSON.stringify(payload).length;
      for (const path in payload) rt.applyPath(this.data, path, payload[path]);
    },
    __derive() {
      return derive ? derive.call(this) : {};
    },
    __set(p, v) {
      rt.set(this, p, v);
    },
  };
  return { page, stats };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

async function runMist() {
  const { page, stats } = makePage({ filter: 'all', todos: makeTodos(), visible: null }, function () {
    const __o = {};
    rt.derive(this, __o, 'visible', 'id', () =>
      this.data.filter === 'all' ? this.data.todos : this.data.todos.filter((t) => !t.done)
    );
    return __o;
  });
  rt.init(page);
  const initBytes = stats.bytes;

  for (let i = 0; i < TOGGLES; i++) {
    const idx = (i * 7) % N;
    // exactly what `todos.value[i].done = !todos.value[i].done` compiles to
    page.__set(`todos[${idx}].done`, !page.data.todos[idx].done);
    await tick();
  }
  const toggleBytes = stats.bytes - initBytes;
  const toggleCalls = stats.calls - 1;

  // filter switch: derived list shrinks → one full write of the filtered array
  page.__set('filter', 'open');
  await tick();
  return { initBytes, toggleBytes, toggleCalls, filterBytes: stats.bytes - initBytes - toggleBytes };
}

async function runHandOptimal() {
  // no derived list: renders `todos` directly with wx:if per row (the trick a
  // careful human uses to avoid resending a filtered copy)
  const { page, stats } = makePage({ filter: 'all', todos: makeTodos() });
  page.setData({ todos: page.data.todos, filter: 'all' });
  const initBytes = stats.bytes;

  for (let i = 0; i < TOGGLES; i++) {
    const idx = (i * 7) % N;
    page.setData({ [`todos[${idx}].done`]: !page.data.todos[idx].done });
  }
  const toggleBytes = stats.bytes - initBytes;
  const toggleCalls = stats.calls - 1;

  page.setData({ filter: 'open' });
  return { initBytes, toggleBytes, toggleCalls, filterBytes: stats.bytes - initBytes - toggleBytes };
}

async function runNaive() {
  // the common lazy pattern: recompute + resend everything
  const { page, stats } = makePage({ filter: 'all', todos: makeTodos(), visible: null });
  const recompute = () => {
    const visible = page.data.filter === 'all' ? page.data.todos : page.data.todos.filter((t) => !t.done);
    page.setData({ todos: page.data.todos, visible });
  };
  recompute();
  const initBytes = stats.bytes;

  for (let i = 0; i < TOGGLES; i++) {
    const idx = (i * 7) % N;
    page.data.todos[idx].done = !page.data.todos[idx].done;
    recompute();
  }
  const toggleBytes = stats.bytes - initBytes;
  const toggleCalls = stats.calls - 1;

  page.data.filter = 'open';
  recompute();
  return { initBytes, toggleBytes, toggleCalls, filterBytes: stats.bytes - initBytes - toggleBytes };
}

function fmt(n) {
  if (n >= 1024 * 1024) return (n / 1024 / 1024).toFixed(1) + ' MB';
  if (n >= 1024) return (n / 1024).toFixed(1) + ' KB';
  return n + ' B';
}

(async () => {
  const results = {
    mist: await runMist(),
    'hand-optimal': await runHandOptimal(),
    naive: await runNaive(),
  };

  const fs = require('fs');
  const rtSize = fs.statSync(require.resolve('../runtime/mist-rt.js')).size;

  console.log(`list size: ${N} rows, ${TOGGLES} toggles\n`);
  console.log(
    'impl'.padEnd(14) +
      'setData calls'.padEnd(15) +
      'bytes/toggle'.padEnd(14) +
      'toggle total'.padEnd(14) +
      'filter switch'
  );
  for (const [name, r] of Object.entries(results)) {
    console.log(
      name.padEnd(14) +
        String(r.toggleCalls).padEnd(15) +
        fmt(Math.round(r.toggleBytes / TOGGLES)).padEnd(14) +
        fmt(r.toggleBytes).padEnd(14) +
        fmt(r.filterBytes)
    );
  }
  console.log(`\nmist runtime shipped in the package: ${fmt(rtSize)} (unminified)`);
  const ratio = results.naive.toggleBytes / results.mist.toggleBytes;
  console.log(`bridge traffic per toggle, naive vs mist: ${ratio.toFixed(0)}x`);

  // machine-readable line for tests
  console.log(
    `RESULT ${JSON.stringify({
      mistToggleAvg: Math.round(results.mist.toggleBytes / TOGGLES),
      optimalToggleAvg: Math.round(results['hand-optimal'].toggleBytes / TOGGLES),
      naiveToggleAvg: Math.round(results.naive.toggleBytes / TOGGLES),
      mistCalls: results.mist.toggleCalls,
    })}`
  );
})();

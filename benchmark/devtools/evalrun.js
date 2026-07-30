// Measures setData cost per interaction by invoking the page's own handler via
// evaluate() — automator's element $$() hangs on form pages, evaluate() does not.
// Same instrument for both frameworks: hook setData, drive N interactions, report.
const automator = require('miniprogram-automator');
const path = require('path');
const CLI = '/Applications/wechatwebdevtools.app/Contents/MacOS/cli';

const project = process.argv[2];
const kind = process.argv[3] || 'mist';        // mist | taro
const N = Number(process.env.N || 30);

const guard = setTimeout(() => { console.log('TIMEOUT'); process.exit(1); }, 200000);

(async () => {
  const mp = await automator.launch({ cliPath: CLI, projectPath: path.resolve(project) });
  let page = await mp.currentPage();
  const want = process.env.WANT_PAGE;
  if (want && !(page.path || '').includes(want)) {
    await mp.reLaunch('/' + want).catch(() => {});
    await new Promise((r) => setTimeout(r, 2500));
    page = await mp.currentPage();
  }
  await page.waitFor(1500);

  const info = await mp.evaluate(() => {
    const p = getCurrentPages()[0];
    return { path: p.route || p.__route__, keys: Object.keys(p.data).join(','), bytes: JSON.stringify(p.data).length };
  });

  const res = await mp.evaluate(function (kind, n) {
    const p = getCurrentPages()[0];
    const b = { calls: 0, bytes: 0, max: 0, samples: [] };
    const orig = p.setData;
    p.setData = function (d, cb) {
      const s = JSON.stringify(d).length;
      b.calls++; b.bytes += s;
      if (s > b.max) b.max = s;
      if (b.samples.length < 2) b.samples.push(JSON.stringify(d).slice(0, 160));
      return orig.call(this, d, cb);
    };
    // one interaction per tick — each gets its own flush, matching a real tap
    const cats = ['Food', 'Transit', 'Fun'];
    const t0 = Date.now();
    return new Promise((resolve) => {
      let i = 0;
      const step = () => {
        if (i >= n) {
          setTimeout(() => { p.setData = orig; resolve({ b: b, ms: Date.now() - t0 }); }, 400);
          return;
        }
        const c = cats[i % 3];
        if (kind === 'mist') { p.pick(c); } else if (p.__setCat) { p.__setCat(c); }
        i++;
        setTimeout(step, 40);
      };
      step();
    });
  }, kind, N);

  console.log('RESULT ' + JSON.stringify({
    project: project, page: info.path, dataKeys: info.keys, initialDataBytes: info.bytes,
    interactions: N, setDataCalls: res.b.calls,
    bytesPerInteraction: res.b.calls ? Math.round(res.b.bytes / N) : 0,
    totalBytes: res.b.bytes, maxPayload: res.b.max, logicMs: res.ms, samples: res.b.samples,
  }));
  clearTimeout(guard);
  await mp.close();
  process.exit(0);
})().catch((e) => { clearTimeout(guard); console.log('ERR:', e.message); process.exit(1); });

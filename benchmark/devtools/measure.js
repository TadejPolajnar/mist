// Framework-agnostic on-device-class benchmark, driven through WeChat DevTools
// via the official miniprogram-automator. Measures the SAME things the same way
// for any project (mist, Taro, native): setData calls + payload bytes (hooked at
// the page object, outside any framework), wall-clock for a scripted interaction
// run, launch/render entries from wx.getPerformance(), and package size.
//
// Usage:
//   node measure.js <project-path> [row-selector]
//   node measure.js mist-app                 # defaults: .bench-row, 50 toggles
//   TOGGLES=100 node measure.js ../taro-app .bench-row
//
// Prerequisites (one-time, manual):
//   - WeChat DevTools installed
//   - DevTools → Settings → Security → enable "Service Port" (服务端口)

const automator = require('miniprogram-automator');
const path = require('path');
const fs = require('fs');

const projectPath = path.resolve(process.argv[2] || 'mist-app');
const ROW_SELECTOR = process.argv[3] || '.bench-row';
const TOGGLES = Number(process.env.TOGGLES || 50);
const CLI_PATH =
  process.env.WX_CLI || '/Applications/wechatwebdevtools.app/Contents/MacOS/cli';

function dirSize(dir) {
  let total = 0;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, entry.name);
    if (entry.isDirectory()) total += dirSize(p);
    else total += fs.statSync(p).size;
  }
  return total;
}

async function main() {
  const miniProgram = await automator.launch({ cliPath: CLI_PATH, projectPath });
  try {
    if (process.env.PAGE) await miniProgram.reLaunch(process.env.PAGE);
    const page = await miniProgram.currentPage();
    await page.waitFor(1000);

    // hook setData on the live page instance — outside any framework
    await miniProgram.evaluate(() => {
      const app = getApp();
      app.__bridge = { calls: 0, bytes: 0, maxPayload: 0 };
      const p = getCurrentPages()[0];
      const orig = p.setData;
      p.setData = function (data, cb) {
        const size = JSON.stringify(data).length;
        app.__bridge.calls++;
        app.__bridge.bytes += size;
        if (size > app.__bridge.maxPayload) app.__bridge.maxPayload = size;
        return orig.call(this, data, cb);
      };
    });

    // 'host|inner' taps .inner inside each custom-component host (component
    // host taps don't reach inner handlers, and '>>>' isn't supported here)
    let rows;
    if (ROW_SELECTOR.includes('|')) {
      const [host, inner] = ROW_SELECTOR.split('|');
      const hosts = await page.$$(host.trim());
      rows = (await Promise.all(hosts.map((h) => h.$(inner.trim())))).filter(Boolean);
    } else {
      rows = await page.$$(ROW_SELECTOR);
    }
    if (rows.length === 0) {
      throw new Error(`no elements matched '${ROW_SELECTOR}' — pass the row selector as argv[3]`);
    }

    // size of the data the renderer holds after first paint — the initial payload proxy
    const initialDataBytes = await miniProgram.evaluate(
      () => JSON.stringify(getCurrentPages()[0].data).length
    );

    const tapMs = [];
    for (let i = 0; i < TOGGLES; i++) {
      const t = Date.now();
      await rows[(i * 7) % rows.length].tap();
      tapMs.push(Date.now() - t);
    }
    await page.waitFor(300);
    const wallMs = tapMs.reduce((a, b) => a + b, 0);
    tapMs.sort((a, b) => a - b);
    const pct = (p) => tapMs[Math.min(tapMs.length - 1, Math.floor((p / 100) * tapMs.length))];

    const bridge = await miniProgram.evaluate(() => getApp().__bridge);

    // filter switch: a structural change (derived list shrinks/grows)
    const beforeFilter = { ...bridge };
    const filterBtn = await page.$('.bench-filter');
    const tf = Date.now();
    if (filterBtn) await filterBtn.tap();
    await page.waitFor(300);
    const filterMs = filterBtn ? Date.now() - tf - 300 : null;
    const afterFilter = await miniProgram.evaluate(() => getApp().__bridge);

    const perfEntries = await miniProgram.evaluate(() => {
      const pick = (list) =>
        (list || []).map((e) => ({
          name: e.name,
          entryType: e.entryType,
          duration: e.duration,
          startTime: e.startTime,
        }));
      // prefer an app-installed observer buffer (mist: App({ __perf }); taro twin: same)
      try {
        const buffered = getApp().__perf;
        if (buffered && buffered.length) return pick(buffered);
      } catch (e) {}
      try {
        return pick(wx.getPerformance().getEntries());
      } catch (e) {
        return [];
      }
    });

    // package size: the miniprogramRoot dir if project.config.json declares one
    let pkgDir = projectPath;
    try {
      const cfg = JSON.parse(fs.readFileSync(path.join(projectPath, 'project.config.json'), 'utf8'));
      if (cfg.miniprogramRoot) pkgDir = path.join(projectPath, cfg.miniprogramRoot);
    } catch (e) {}

    const report = {
      project: projectPath,
      rows: rows.length,
      toggles: TOGGLES,
      msPerToggle: Math.round((wallMs / TOGGLES) * 10) / 10,
      toggleP50Ms: pct(50),
      toggleP95Ms: pct(95),
      setDataCalls: beforeFilter.calls,
      bytesPerToggle: Math.round(beforeFilter.bytes / TOGGLES),
      maxPayloadBytes: beforeFilter.maxPayload,
      initialDataBytes,
      filterSwitch: filterBtn
        ? { ms: filterMs, bytes: afterFilter.bytes - beforeFilter.bytes, calls: afterFilter.calls - beforeFilter.calls }
        : null,
      packageBytes: dirSize(pkgDir),
      launch: perfEntries,
    };
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await miniProgram.close();
  }
}

main().catch((e) => {
  console.error('measure failed:', e.message);
  console.error(
    '\nchecklist: WeChat DevTools installed? Service Port enabled (Settings → Security)? project path has project.config.json?'
  );
  process.exit(1);
});

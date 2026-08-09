const automator = require('miniprogram-automator');
const path = require('path');

const CLI = '/Applications/wechatwebdevtools.app/Contents/MacOS/cli';
const ROOT = path.resolve(__dirname, '../../examples');

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

async function shoot(name) {
  console.log(`${name}: launching…`);
  const mp = await automator.launch({ cliPath: CLI, projectPath: path.join(ROOT, name) });
  try {
    const page = await mp.currentPage();
    console.log(`${name}: page ${page.path}`);
    await wait(6000);
    const out = path.join(ROOT, name, 'screenshot.png');
    await mp.screenshot({ path: out });
    console.log(`${name}: saved ${out}`);
  } finally {
    await mp.close().catch(() => {});
  }
}

async function main() {
  for (const name of ['food', 'portfolio', 'kanban']) {
    await shoot(name);
  }
}

main().then(
  () => {
    console.log('SHOTS OK');
    process.exit(0);
  },
  (e) => {
    console.error('SHOTS FAILED:', e.message);
    process.exit(1);
  }
);

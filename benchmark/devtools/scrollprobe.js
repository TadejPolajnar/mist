const automator = require('miniprogram-automator');
const path = require('path');

const CLI = '/Applications/wechatwebdevtools.app/Contents/MacOS/cli';
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

function assert(cond, msg) {
  if (!cond) {
    throw new Error(msg);
  }
}

(async () => {
  const mp = await automator.launch({
    cliPath: CLI,
    projectPath: path.resolve(__dirname, '../../examples/feed'),
    timeout: 120000,
  });
  let page = null;
  for (let i = 0; i < 20 && !page; i++) {
    await wait(1500);
    page = await mp.currentPage().catch(() => null);
  }
  assert(page, 'feed page never became ready');
  console.log('page:', page.path);
  await wait(3000);
  await mp.screenshot({ path: path.resolve(__dirname, '../../examples/feed/screenshot.png') });
  console.log('top screenshot saved');

  const shownCount = () =>
    mp.evaluate(() => {
      const pages = getCurrentPages();
      return pages[pages.length - 1].data.visible.length;
    });

  const before = await shownCount();
  console.log('visible before scroll:', before);
  assert(before === 50, 'expected 50 initial rows, got ' + before);

  await mp.pageScrollTo(200000);
  await wait(1500);
  const afterOne = await shownCount();
  console.log('visible after first scroll:', afterOne);
  assert(afterOne === 100, 'reach-bottom did not page: ' + afterOne);

  await mp.pageScrollTo(400000);
  await wait(1500);
  const afterTwo = await shownCount();
  console.log('visible after second scroll:', afterTwo);
  assert(afterTwo === 150, 'second reach-bottom did not page: ' + afterTwo);

  await mp.screenshot({ path: path.resolve(__dirname, '../../examples/feed/screenshot-scrolled.png') });
  console.log('SCROLL OK');
  await mp.close();
})().then(
  () => process.exit(0),
  (e) => {
    console.error('SCROLL FAILED:', e.message);
    process.exit(1);
  }
);

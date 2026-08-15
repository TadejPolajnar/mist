const assert = require('node:assert');

module.exports = async () => {
  const app = bootPage('feed');
  await flush();

  assert.ok(!('posts' in app.data()), 'posts must be dead-data eliminated');
  assert.equal(app.page._posts.length, 1000);
  assert.equal(app.data().visible.length, 50);

  const target = app.data().visible[2];
  const before = target.likes;
  app.page.toggleLike(target.id);
  await flush();
  assert.equal(app.data().visible[2].likes, before + 1);
  assert.ok(app.lastPatch().size < 200, `like patch too large: ${app.lastPatch().size} bytes`);

  const lab = load('stores/lab');
  lab.setFullRender(true);
  await flush(30);
  assert.ok(app.rejected.length >= 1, 'oversized setData must be rejected');
  assert.equal(app.data().visible.length, 50, 'rejected setData must roll back');

  lab.setFullRender(false);
  await flush(30);
  app.page.toggleLike(app.data().visible[3].id);
  await flush(30);
  assert.equal(app.data().visible.length, 50, 'must recover after rollback');
  assert.ok(app.lastPatch().size < 200, `post-recovery patch too large: ${app.lastPatch().size} bytes`);
};

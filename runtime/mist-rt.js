// mist runtime: setData batching + keyed diff for derived arrays

function rootOf(path) {
  let i = 0;
  while (i < path.length && path[i] !== '.' && path[i] !== '[') i++;
  return path.slice(0, i);
}

function markDirty(page, name) {
  if (name == null) {
    page.__dirty = null;
    page.__dirtyAll = true;
    return;
  }
  if (page.__dirtyAll) return;
  (page.__dirty || (page.__dirty = new Set())).add(name);
}

function set(page, path, value) {
  if (!page.__pending) {
    page.__pending = {};
    Promise.resolve().then(() => flush(page));
  }
  markDirty(page, rootOf(path));
  page.__pending[path] = value;
}

let budget = 900 * 1024;

function setDataBudget(bytes) {
  budget = bytes;
}

function utf8Len(s) {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c < 0x80) n += 1;
    else if (c < 0x800) n += 2;
    else if (
      c >= 0xd800 &&
      c < 0xdc00 &&
      i + 1 < s.length &&
      s.charCodeAt(i + 1) >= 0xdc00 &&
      s.charCodeAt(i + 1) < 0xe000
    ) {
      n += 4;
      i++;
    } else n += 3;
  }
  return n;
}

function send(page, payload) {
  const keys = Object.keys(payload);
  if (keys.length < 2) {
    page.setData(payload);
    return;
  }
  let total = 0;
  let oversized = false;
  const sizes = keys.map((k) => {
    const v = payload[k];
    const s = k.length + (v === undefined ? 9 : utf8Len(JSON.stringify(v))) + 6;
    if (s > budget) oversized = true;
    total += s;
    return s;
  });
  if (total <= budget || oversized) {
    page.setData(payload);
    return;
  }
  let chunk = {};
  let chunkSize = 0;
  for (let i = 0; i < keys.length; i++) {
    if (chunkSize > 0 && chunkSize + sizes[i] > budget) {
      page.setData(chunk);
      chunk = {};
      chunkSize = 0;
    }
    chunk[keys[i]] = payload[keys[i]];
    chunkSize += sizes[i];
  }
  page.setData(chunk);
}

function flush(page) {
  const pending = page.__pending;
  page.__pending = null;
  if (!pending) return;
  const undoPaths = [];
  for (const path in pending) {
    undoPaths.push([path, applyPathCapture(page.data, path, pending[path])]);
  }
  page.__undo = [];
  Object.assign(pending, page.__derive());
  const undo = page.__undo;
  page.__undo = null;
  try {
    send(page, pending);
    page.__dirty = null;
    page.__dirtyAll = false;
    page.__resync = false;
  } catch (e) {
    for (let i = undo.length - 1; i >= 0; i--) {
      const u = undo[i];
      page.data[u[0]] = u[1];
      if (u[2]) page.__prev[u[0]] = u[3];
      else delete page.__prev[u[0]];
    }
    for (let i = undoPaths.length - 1; i >= 0; i--) {
      unapplyPath(page.data, undoPaths[i][0], undoPaths[i][1]);
    }
    page.__dirty = null;
    page.__dirtyAll = true;
    if (page.__storePairs && page.__storePairs.length > 0 && !page.__resync) {
      page.__resync = true;
      for (const pair of page.__storePairs) {
        set(page, pair[1], pair[0].value);
      }
    }
    console.error('mist: setData rejected — state rolled back; split large writes into smaller batches', e);
  }
}

function touch(page, name) {
  if (!page.__pending) {
    page.__pending = {};
    Promise.resolve().then(() => flush(page));
  }
  markDirty(page, name);
}

function init(page) {
  const seed = page.__derive();
  if (Object.keys(seed).length) send(page, seed);
}

const pathSegs = new Map();

// `todos[3].done` → ['todos', 3, 'done']
function parsePath(path) {
  const hit = pathSegs.get(path);
  if (hit) return hit;
  const segs = [];
  let cur = '';
  for (let i = 0; i < path.length; i++) {
    const c = path[i];
    if (c === '.') {
      if (cur) segs.push(cur);
      cur = '';
    } else if (c === '[') {
      if (cur) segs.push(cur);
      cur = '';
      let num = '';
      i++;
      while (i < path.length && path[i] !== ']') {
        num += path[i];
        i++;
      }
      segs.push(Number(num));
    } else {
      cur += c;
    }
  }
  if (cur) segs.push(cur);
  if (pathSegs.size >= 4096) pathSegs.clear();
  pathSegs.set(path, segs);
  return segs;
}

function unapplyPath(obj, path, value) {
  const segs = parsePath(path);
  let cur = obj;
  for (let i = 0; i < segs.length - 1; i++) {
    if (cur == null) return;
    cur = cur[segs[i]];
  }
  if (cur == null) return;
  const last = segs[segs.length - 1];
  if (value === undefined) {
    if (Array.isArray(cur) && typeof last === 'number' && last === cur.length - 1) {
      cur.length = last;
    } else {
      delete cur[last];
    }
  } else {
    cur[last] = value;
  }
}

function applyPathCapture(obj, path, value) {
  const segs = parsePath(path);
  let cur = obj;
  for (let i = 0; i < segs.length - 1; i++) {
    const s = segs[i];
    if (cur[s] == null) cur[s] = typeof segs[i + 1] === 'number' ? [] : {};
    cur = cur[s];
  }
  const last = segs[segs.length - 1];
  const prior = cur[last];
  cur[last] = value;
  return prior;
}

// walk/create and assign on the local data mirror
function applyPath(obj, path, value) {
  const segs = parsePath(path);
  let cur = obj;
  for (let i = 0; i < segs.length - 1; i++) {
    const s = segs[i];
    if (cur[s] == null) cur[s] = typeof segs[i + 1] === 'number' ? [] : {};
    cur = cur[s];
  }
  cur[segs[segs.length - 1]] = value;
}

/**
 * Recompute one derived value into `out` with minimal writes.
 * Keyed arrays: same key sequence → per-index writes for shallow-changed items;
 * anything else (reorder/insert/remove) → whole-key write. Skips the write
 * entirely when nothing changed.
 */
function derive(page, out, name, key, compute, deps) {
  const dirty = page.__dirty;
  if (dirty && deps && !page.__dirtyAll && !deps.some((d) => dirty.has(d))) return;
  const next = compute();
  const prevStore = page.__prev || (page.__prev = {});
  const prev = prevStore[name];
  if (page.__undo) {
    page.__undo.push([name, page.data[name], name in prevStore, prev]);
  }
  let wrote = false;

  if (Array.isArray(next) && key) {
    const snap = new Array(next.length);
    const sameShape =
      Array.isArray(prev) && prev.length === next.length && sameKeys(prev, next, key);
    if (!sameShape) {
      out[name] = next;
      wrote = true;
      for (let i = 0; i < next.length; i++) snap[i] = snapRow(next[i]);
    } else {
      for (let i = 0; i < next.length; i++) {
        const p = prev[i];
        const n = next[i];
        // nested-hoist rows (_hl over a nested loop) are arrays rebuilt every
        // recompute — compare items, not the always-fresh row reference
        if (Array.isArray(n)) {
          if (Array.isArray(p) && p.length === n.length && n.every((v, j) => shallowEq(p[j], v))) {
            snap[i] = p;
            continue;
          }
          snap[i] = snapRow(n);
          out[name + '[' + i + ']'] = n;
          wrote = true;
          continue;
        }
        if (shallowEq(p, n)) {
          snap[i] = p;
          continue;
        }
        snap[i] = snapshot(n);
        // field-level diff: same-shape objects emit only the changed fields
        if (p && n && typeof p === 'object' && typeof n === 'object' && !Array.isArray(n)) {
          const pk = Object.keys(p);
          if (pk.length === Object.keys(n).length && pk.every((k) => k in n)) {
            for (const k of pk) {
              if (p[k] !== n[k]) {
                out[name + '[' + i + '].' + k] = n[k];
                wrote = true;
              }
            }
            continue;
          }
        }
        out[name + '[' + i + ']'] = n;
        wrote = true;
      }
    }
    page.data[name] = next;
    prevStore[name] = snap;
    if (dirty && wrote) dirty.add(name);
    return;
  }

  if (Array.isArray(next)) {
    if (!Array.isArray(prev) || prev.length !== next.length || !next.every((v, i) => shallowEq(prev[i], v))) {
      out[name] = next;
      wrote = true;
    }
    page.data[name] = next;
    prevStore[name] = next.map(snapshot);
    if (dirty && wrote) dirty.add(name);
    return;
  }

  if (!(name in prevStore) || !shallowEq(prev, next)) {
    out[name] = next;
    wrote = true;
  }
  page.data[name] = next;
  prevStore[name] = snapshot(next);
  if (dirty && wrote) dirty.add(name);
}

function sameKeys(prev, next, key) {
  for (let i = 0; i < next.length; i++) {
    const a = key === '*this' ? prev[i] : prev[i] && prev[i][key];
    const b = key === '*this' ? next[i] : next[i] && next[i][key];
    if (a !== b) return false;
  }
  return true;
}

function shallowEq(a, b) {
  if (a === b) return true;
  if (a == null || b == null || typeof a !== 'object' || typeof b !== 'object') return false;
  const ka = Object.keys(a);
  const kb = Object.keys(b);
  if (ka.length !== kb.length) return false;
  for (const k of ka) {
    if (a[k] !== b[k]) return false;
  }
  return true;
}

function snapshot(v) {
  if (v && typeof v === 'object' && !Array.isArray(v)) return Object.assign({}, v);
  return v;
}

function snapRow(v) {
  if (Array.isArray(v)) return v.map(snapshot);
  return snapshot(v);
}

/**
 * Shared reactive state across pages. `__set(path, value)` applies the write to
 * the store's own value and notifies every subscriber with the same path — each
 * live page turns that into one path-precise, batched setData on its mirror key.
 */
function store(init, opts) {
  const persistKey = opts && opts.persist;
  const version = (opts && opts.version) || 0;
  const hasWx = typeof wx !== 'undefined' && wx && typeof wx.setStorageSync === 'function';
  let value = init;
  if (persistKey && hasWx) {
    try {
      const saved = wx.getStorageSync(persistKey);
      if (saved && typeof saved === 'object' && 'data' in saved) {
        if (saved.v === version) {
          value = saved.data;
        } else if (opts.migrate) {
          const migrated = opts.migrate(saved.data, saved.v);
          if (migrated === undefined) {
            value = init;
          } else {
            value = migrated;
            // persist immediately so migrate never re-runs on the next launch
            wx.setStorageSync(persistKey, { v: version, data: value });
          }
        }
      }
    } catch (e) {
      value = init;
    }
  }
  function persistNow() {
    box.__persistTimer = null;
    try {
      wx.setStorageSync(persistKey, { v: version, data: box.value });
    } catch (e) {}
  }
  const box = {
    value,
    __subs: new Set(),
    __set(path, value) {
      if (path == null) {
        this.value = value;
      } else {
        applyPath(this, 'value.' + path, value);
      }
      for (const fn of this.__subs) fn(path, value);
      if (persistKey && hasWx) {
        if (box.__persistTimer) clearTimeout(box.__persistTimer);
        box.__persistTimer = setTimeout(persistNow, 200);
      }
    },
    subscribe(fn) {
      this.__subs.add(fn);
      return () => this.__subs.delete(fn);
    },
  };
  if (persistKey && hasWx && typeof wx.onAppHide === 'function') {
    // flush a pending debounced write before the app backgrounds
    wx.onAppHide(() => {
      if (box.__persistTimer) {
        clearTimeout(box.__persistTimer);
        persistNow();
      }
    });
  }
  return box;
}

/// pairs: [[storeBox, 'mirrorKey'], ...]
function bindStores(page, pairs) {
  const seed = {};
  page.__storePairs = (page.__storePairs || []).concat(pairs);
  page.__storeUnsubs = (page.__storeUnsubs || []).concat(
    pairs.map(([s, key]) => {
      seed[key] = s.value;
      return s.subscribe((path, value) => {
        if (path == null) {
          set(page, key, value);
        } else {
          set(page, key + (path.startsWith('[') ? path : '.' + path), value);
        }
      });
    })
  );
  page.setData(seed);
}

function unbindStores(page) {
  (page.__storeUnsubs || []).forEach((u) => u());
  page.__storeUnsubs = [];
  page.__storePairs = [];
}

/**
 * Launch/render metrics: call as the first statement of app.js so the observer
 * exists before launch entries fire. Entries accumulate in `perfEntries`
 * (exported by reference — attach it to App({ __perf: rt.perfEntries }) to make
 * it reachable via getApp() from tooling).
 */
const perfEntries = [];
function observePerf() {
  try {
    const perf = wx.getPerformance();
    // pick up anything already buffered, then observe the rest
    try {
      perfEntries.push(...perf.getEntries());
    } catch (e) {}
    const obs = perf.createObserver((list) => {
      perfEntries.push(...list.getEntries());
    });
    obs.observe({ entryTypes: ['navigation', 'render', 'script'] });
  } catch (e) {}
}

module.exports = {
  set,
  touch,
  flush,
  setDataBudget,
  init,
  derive,
  applyPath,
  store,
  bindStores,
  unbindStores,
  observePerf,
  perfEntries,
};

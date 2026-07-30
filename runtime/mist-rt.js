
function set(page, path, value) {
  if (!page.__pending) {
    page.__pending = {};
    Promise.resolve().then(() => flush(page));
  }
  page.__pending[path] = value;
}

function flush(page) {
  const pending = page.__pending;
  page.__pending = null;
  if (!pending) return;
  for (const path in pending) applyPath(page.data, path, pending[path]);
  Object.assign(pending, page.__derive());
  page.setData(pending);
}

function touch(page) {
  if (!page.__pending) {
    page.__pending = {};
    Promise.resolve().then(() => flush(page));
  }
}

function init(page) {
  page.setData(page.__derive());
}

function parsePath(path) {
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
  return segs;
}

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

function derive(page, out, name, key, compute) {
  const next = compute();
  const prevStore = page.__prev || (page.__prev = {});
  const prev = prevStore[name];

  if (Array.isArray(next) && key) {
    if (!Array.isArray(prev) || prev.length !== next.length || !sameKeys(prev, next, key)) {
      out[name] = next;
    } else {
      for (let i = 0; i < next.length; i++) {
        if (!shallowEq(prev[i], next[i])) {
          const p = prev[i];
          const n = next[i];
          if (p && n && typeof p === 'object' && typeof n === 'object' && !Array.isArray(n)) {
            const pk = Object.keys(p);
            if (pk.length === Object.keys(n).length && pk.every((k) => k in n)) {
              for (const k of pk) {
                if (p[k] !== n[k]) out[name + '[' + i + '].' + k] = n[k];
              }
              continue;
            }
          }
          out[name + '[' + i + ']'] = n;
        }
      }
    }
    page.data[name] = next;
    prevStore[name] = next.map(snapshot);
    return;
  }

  if (Array.isArray(next)) {
    if (!Array.isArray(prev) || prev.length !== next.length || !next.every((v, i) => shallowEq(prev[i], v))) {
      out[name] = next;
    }
    page.data[name] = next;
    prevStore[name] = next.map(snapshot);
    return;
  }

  if (!(name in prevStore) || !shallowEq(prev, next)) {
    out[name] = next;
  }
  page.data[name] = next;
  prevStore[name] = snapshot(next);
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

function store(init) {
  return {
    value: init,
    __subs: new Set(),
    __set(path, value) {
      if (path == null) {
        this.value = value;
      } else {
        applyPath(this, 'value.' + path, value);
      }
      for (const fn of this.__subs) fn(path, value);
    },
    subscribe(fn) {
      this.__subs.add(fn);
      return () => this.__subs.delete(fn);
    },
  };
}

function bindStores(page, pairs) {
  const seed = {};
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
}

const perfEntries = [];
function observePerf() {
  try {
    const perf = wx.getPerformance();
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
  init,
  derive,
  applyPath,
  store,
  bindStores,
  unbindStores,
  observePerf,
  perfEntries,
};

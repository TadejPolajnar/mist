'use strict';
const path = require('path');
const fs = require('fs');

const dist = process.env.MIST_DIST;
const testFile = process.argv[2];
if (!dist || !testFile) {
  console.error('usage: MIST_DIST=<dist> node mist-test.js <test-file>');
  process.exit(2);
}

const rt = require(path.join(dist, 'mist-rt.js'));

const storage = new Map();
const wxCalls = [];
const appHideHandlers = [];

const wxBase = {
  getStorageSync: (k) => (storage.has(k) ? storage.get(k) : undefined),
  setStorageSync: (k, v) => {
    storage.set(k, JSON.parse(JSON.stringify(v)));
  },
  removeStorageSync: (k) => {
    storage.delete(k);
  },
  clearStorageSync: () => {
    storage.clear();
  },
  onAppHide: (fn) => {
    appHideHandlers.push(fn);
  },
  __storage: storage,
  __calls: wxCalls,
};

globalThis.wx = new Proxy(wxBase, {
  get(target, prop) {
    if (prop in target) return target[prop];
    if (typeof prop !== 'string') return undefined;
    return (...args) => {
      wxCalls.push({ name: prop, args });
    };
  },
});

let capturedPage = null;
globalThis.App = (o) => {
  globalThis.__app = o;
};
globalThis.Component = (o) => {
  globalThis.__component = o;
};
globalThis.Page = (o) => {
  capturedPage = o;
};
globalThis.getApp = () => globalThis.__app || {};

function pathSegments(key) {
  const segs = [];
  key.replace(/([^[\].]+)|\[(\d+)\]/g, (_, name, idx) => {
    segs.push(name !== undefined ? name : Number(idx));
    return '';
  });
  return segs;
}

function applyPatch(data, patch) {
  for (const [key, value] of Object.entries(patch)) {
    if (value === undefined) continue;
    const segs = pathSegments(key);
    let target = data;
    for (let i = 0; i < segs.length - 1; i++) {
      if (target[segs[i]] == null) {
        target[segs[i]] = typeof segs[i + 1] === 'number' ? [] : {};
      }
      target = target[segs[i]];
    }
    target[segs[segs.length - 1]] = value;
  }
}

function distFile(name) {
  const rel = name.includes('/') ? name : `pages/${name}/${name}`;
  return path.join(dist, rel.endsWith('.js') ? rel : rel + '.js');
}

function componentFile(name) {
  const rel = name.includes('/') ? name : `components/${name}/${name}`;
  return path.join(dist, rel.endsWith('.js') ? rel : rel + '.js');
}

function instrument(target, options) {
  const patches = [];
  const rejected = [];
  const limit = options.setDataLimit ?? 1024 * 1024;
  target.setData = function (patch, cb) {
    const size = Buffer.byteLength(JSON.stringify(patch));
    if (size > limit) {
      rejected.push({ keys: Object.keys(patch), size, patch });
      throw new Error(`setData over limit: ${size} > ${limit} bytes`);
    }
    patches.push({ keys: Object.keys(patch), size, patch });
    applyPatch(this.data, patch);
    if (cb) cb();
  };
  return { patches, rejected };
}

function bootPage(name, options = {}) {
  const file = distFile(name);
  if (!fs.existsSync(file) && fs.existsSync(componentFile(name))) {
    throw new Error(`${name} is a component — use bootComponent('${name}')`);
  }
  capturedPage = null;
  globalThis.__component = null;
  delete require.cache[require.resolve(file)];
  require(file);
  if (!capturedPage) {
    if (globalThis.__component) {
      throw new Error(`${file} registered a Component — use bootComponent('${name}')`);
    }
    throw new Error(`${file} did not register a Page`);
  }
  const page = capturedPage;
  const { patches, rejected } = instrument(page, options);
  if (page.onLoad) page.onLoad(options.query ?? {});
  return {
    page,
    data: () => page.data,
    patches,
    rejected,
    lastPatch: () => patches[patches.length - 1] ?? null,
    totalBytes: () => patches.reduce((n, p) => n + p.size, 0),
  };
}

function bootComponent(name, options = {}) {
  const file = componentFile(name);
  if (!fs.existsSync(file) && fs.existsSync(distFile(name))) {
    throw new Error(`${name} is a page — use bootPage('${name}')`);
  }
  let registered = null;
  capturedPage = null;
  globalThis.__component = null;
  globalThis.Component = (o) => {
    registered = o;
  };
  delete require.cache[require.resolve(file)];
  require(file);
  globalThis.Component = (o) => {
    globalThis.__component = o;
  };
  if (!registered) {
    if (capturedPage) {
      throw new Error(`${name} is a page — use bootPage('${name}')`);
    }
    throw new Error(`${file} did not register a Component`);
  }
  const instance = {
    data: JSON.parse(JSON.stringify(registered.data ?? {})),
  };
  for (const [key, prop] of Object.entries(registered.properties ?? {})) {
    instance.data[key] = options.props?.[key] ?? prop.value;
  }
  Object.assign(instance, registered.methods);
  const { patches, rejected } = instrument(instance, options);
  const events = [];
  instance.triggerEvent = (name, detail, opts) => events.push({ name, detail, opts });
  if (registered.lifetimes?.attached) registered.lifetimes.attached.call(instance);
  if (registered.lifetimes?.ready) registered.lifetimes.ready.call(instance);
  function setProp(key, value) {
    instance.data[key] = value;
    rt.touch(instance, key);
  }
  return {
    comp: instance,
    data: () => instance.data,
    patches,
    rejected,
    lastPatch: () => patches[patches.length - 1] ?? null,
    totalBytes: () => patches.reduce((n, p) => n + p.size, 0),
    events,
    setProp,
  };
}

function flush(ms = 0) {
  return new Promise((resolve) => setTimeout(resolve, ms)).then(
    () => new Promise((resolve) => setTimeout(resolve, ms))
  );
}

function load(name) {
  const file = distFile(name);
  return require(file);
}

function resetModules() {
  for (const key of Object.keys(require.cache)) {
    if (key.startsWith(dist)) delete require.cache[key];
  }
}

function appHide() {
  for (const fn of appHideHandlers) fn();
}

globalThis.bootPage = bootPage;
globalThis.bootComponent = bootComponent;
globalThis.flush = flush;
globalThis.load = load;
globalThis.resetModules = resetModules;
globalThis.appHide = appHide;

function printError(err) {
  const text = err && err.stack ? err.stack : String(err);
  console.error(
    text
      .split('\n')
      .filter((line) => !line.includes('.mist-test-runner.js'))
      .join('\n')
  );
}

process.on('unhandledRejection', (err) => {
  printError(err);
  process.exit(1);
});

(async () => {
  const exported = require(path.resolve(testFile));
  if (typeof exported === 'function') {
    await exported();
  } else {
    await exported;
  }
  await flush();
})().catch((err) => {
  printError(err);
  process.exit(1);
});

use std::fs;
use std::process::Command;

const DRIVER: &str = r#"
const { spawn } = require('node:child_process')
const server = spawn(process.argv[2], [], { stdio: ['pipe', 'pipe', 'inherit'] })
let buf = Buffer.alloc(0)
const pending = new Map()
const diagRounds = []
let diagWaiter = null
const failures = []

server.stdout.on('data', (d) => {
  buf = Buffer.concat([buf, d])
  for (;;) {
    const headerEnd = buf.indexOf('\r\n\r\n')
    if (headerEnd === -1) return
    const len = parseInt(/Content-Length: (\d+)/.exec(buf.slice(0, headerEnd).toString())[1], 10)
    if (buf.length < headerEnd + 4 + len) return
    const msg = JSON.parse(buf.slice(headerEnd + 4, headerEnd + 4 + len).toString())
    buf = buf.slice(headerEnd + 4 + len)
    if (msg.id !== undefined && pending.has(msg.id)) {
      pending.get(msg.id)(msg)
      pending.delete(msg.id)
    } else if (msg.method === 'textDocument/publishDiagnostics') {
      diagRounds.push(msg.params.diagnostics)
      if (diagWaiter) { diagWaiter(); diagWaiter = null }
    }
  }
})

let nextId = 1
function send(method, params, expectReply) {
  const msg = { jsonrpc: '2.0', method, params }
  if (expectReply) msg.id = nextId++
  const body = JSON.stringify(msg)
  server.stdin.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`)
  if (!expectReply) return Promise.resolve()
  return new Promise((resolve) => pending.set(msg.id, resolve))
}
function waitDiag(count) {
  if (diagRounds.length >= count) return Promise.resolve()
  return new Promise((resolve) => { diagWaiter = resolve })
}
function check(name, cond, detail) {
  if (cond) console.log('PASS', name)
  else { failures.push(name); console.log('FAIL', name, ':', detail) }
}

const badSrc = [
  '---',
  "import { state, derived } from 'mist'",
  'const todos = state([])',
  'const open = derived(() => todos.value.filter(t => !t.done))',
  'function toggle(i, done) { todos.value[i].done = done }',
  'function bad() { const t = todos.value[0]; t.done = true }',
  '---',
  '<span onTap={() => toggle(0, true)}>{open.value.length}</span>',
].join('\n') + '\n'
const goodSrc = badSrc.replace('t.done = true', 't.done && 0')
const root = process.argv[3]
const { pathToFileURL } = require('node:url')
const uri = pathToFileURL(root + '/pages/index.mist').href

async function main() {
  const init = await send('initialize', { capabilities: {} }, true)
  const caps = init.result.capabilities
  check('caps', caps.completionProvider && caps.hoverProvider && caps.definitionProvider && caps.signatureHelpProvider && caps.renameProvider && caps.textDocumentSync === 2, JSON.stringify(caps))
  await send('initialized', {})
  await send('textDocument/didOpen', { textDocument: { uri, languageId: 'mist', version: 1, text: badSrc } })
  await waitDiag(1)
  const d = diagRounds[0]
  check('diag-m1001', d.length === 1 && String(d[0].code) === 'M1001' && d[0].range.start.line === 5, JSON.stringify(d))
  await send('textDocument/didChange', { textDocument: { uri, version: 2 }, contentChanges: [{ text: goodSrc }] })
  await waitDiag(2)
  check('diag-clear', diagRounds[1].length === 0, JSON.stringify(diagRounds[1]))
  const comp = await send('textDocument/completion', { textDocument: { uri }, position: { line: 7, character: 37 } }, true)
  const labels = (comp.result || []).map((c) => `${c.label}:${c.detail}`)
  check('completion', labels.includes('todos:state') && labels.includes('open:derived') && labels.includes('toggle:method'), JSON.stringify(labels))
  const tcomp = await send('textDocument/completion', { textDocument: { uri }, position: { line: 7, character: 3 } }, true)
  const tlabels = (tcomp.result || []).map((c) => c.label)
  check('completion-tags', tlabels.includes('scroll-view') && tlabels.includes('div'), JSON.stringify(tlabels.slice(0, 10)))
  const acomp = await send('textDocument/completion', { textDocument: { uri }, position: { line: 7, character: 6 } }, true)
  const alabels = (acomp.result || []).map((c) => `${c.label}:${c.detail}`)
  check('completion-attrs', alabels.includes('user-select:attribute') && alabels.includes('onLongPress:event'), JSON.stringify(alabels.slice(0, 12)))
  const hover = await send('textDocument/hover', { textDocument: { uri }, position: { line: 7, character: 20 } }, true)
  check('hover', hover.result && hover.result.contents.value.includes('**toggle**(i, done)'), JSON.stringify(hover.result))
  const def = await send('textDocument/definition', { textDocument: { uri }, position: { line: 7, character: 20 } }, true)
  check('definition', def.result && def.result.range.start.line === 4, JSON.stringify(def.result))
  const sig = await send('textDocument/signatureHelp', { textDocument: { uri }, position: { line: 7, character: 27 } }, true)
  check('sighelp', sig.result && sig.result.signatures[0].label === 'toggle(i, done)' && sig.result.activeParameter === 0, JSON.stringify(sig.result))
  const ren = await send('textDocument/rename', { textDocument: { uri }, position: { line: 7, character: 20 }, newName: 'flip' }, true)
  const edits = ren.result && ren.result.changes && ren.result.changes[uri]
  check('rename', edits && edits.length >= 2, JSON.stringify(ren.result))
  const bad = await send('textDocument/rename', { textDocument: { uri }, position: { line: 7, character: 20 }, newName: '9bad' }, true)
  check('rename-invalid', bad.error && bad.error.message.includes('identifier'), JSON.stringify(bad))

  const badLine = goodSrc.split('\n')[5]
  const col = badLine.indexOf('&& 0')
  await send('textDocument/didChange', { textDocument: { uri, version: 3 }, contentChanges: [
    { range: { start: { line: 5, character: col }, end: { line: 5, character: col + 4 } }, text: '= true' },
  ] })
  await waitDiag(3)
  const d3 = diagRounds[2]
  check('diag-incremental', d3.length === 1 && String(d3[0].code) === 'M1001', JSON.stringify(d3))

  const storeUri = pathToFileURL(root + '/pages/store.mist').href
  const storeSrc = require('node:fs').readFileSync(root + '/pages/store.mist', 'utf8')
  await send('textDocument/didOpen', { textDocument: { uri: storeUri, languageId: 'mist', version: 1, text: storeSrc } })
  await waitDiag(4)
  const xr = await send('textDocument/rename', { textDocument: { uri: storeUri }, position: { line: 2, character: 18 }, newName: 'record' }, true)
  const changed = xr.result && xr.result.changes ? Object.keys(xr.result.changes) : []
  const storeFile = changed.find((k) => k.endsWith('stats.ts'))
  const pageFile = changed.find((k) => k.endsWith('store.mist'))
  check('rename-cross-file', changed.length === 2 && storeFile && pageFile && xr.result.changes[pageFile].length === 2 && xr.result.changes[storeFile].length >= 1, JSON.stringify(xr.result))

  console.log(failures.length === 0 ? 'ALL OK' : `FAILED ${failures.length}`)
  server.kill()
  process.exit(failures.length === 0 ? 0 : 1)
}

setTimeout(() => { console.log('FAIL timeout'); server.kill(); process.exit(1) }, 20000)
main()
"#;

#[test]
fn lsp_end_to_end_over_stdio() {
    let dir = std::env::temp_dir().join("mist-lsp-e2e");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("pages")).expect("temp dir");
    fs::create_dir_all(dir.join("stores")).expect("temp dir");
    fs::write(dir.join("app.mist"), "---\n---\n").expect("write app");
    fs::write(
        dir.join("stores/stats.ts"),
        "import { store } from 'mist'\nexport const cart = store({ n: 0 })\nexport function track() { cart.value.n++ }\n",
    )
    .expect("write store");
    fs::write(
        dir.join("pages/store.mist"),
        "---\nimport { cart, track } from '../stores/stats.ts'\nfunction go() { track() }\n---\n<span onTap={go}>{cart.value.n}</span>\n",
    )
    .expect("write store page");
    let driver = dir.join("driver.js");
    fs::write(&driver, DRIVER).expect("write driver");
    let bin = env!("CARGO_BIN_EXE_mistc-lsp");
    let out = match Command::new("node").arg(&driver).arg(bin).arg(&dir).output() {
        Ok(o) => o,
        Err(_) => {
            eprintln!("skipping lsp e2e: node not available");
            return;
        }
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("ALL OK"),
        "lsp e2e failed:\n{}\n{}",
        stdout,
        String::from_utf8_lossy(&out.stderr)
    );
}

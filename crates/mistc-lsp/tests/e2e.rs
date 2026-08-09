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
const uri = 'file:///tmp/mist-lsp-e2e/pages/index.mist'

async function main() {
  const init = await send('initialize', { capabilities: {} }, true)
  const caps = init.result.capabilities
  check('caps', caps.completionProvider && caps.hoverProvider && caps.definitionProvider && caps.signatureHelpProvider && caps.renameProvider && caps.textDocumentSync === 1, JSON.stringify(caps))
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
    fs::create_dir_all(&dir).expect("temp dir");
    let driver = dir.join("driver.js");
    fs::write(&driver, DRIVER).expect("write driver");
    let bin = env!("CARGO_BIN_EXE_mistc-lsp");
    let out = match Command::new("node").arg(&driver).arg(bin).output() {
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

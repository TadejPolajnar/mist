const { workspace, window } = require('vscode')
const { LanguageClient } = require('vscode-languageclient/node')

let client

function activate() {
  const command = workspace.getConfiguration('mist').get('lspPath') || 'mistc-lsp'
  client = new LanguageClient(
    'mist',
    'Mist Language Server',
    { command },
    { documentSelector: [{ language: 'mist' }] }
  )
  client.start().catch(() => {
    client = undefined
    window.setStatusBarMessage('mistc-lsp not found — highlighting only (set mist.lspPath)', 10000)
  })
}

function deactivate() {
  if (client) {
    return client.stop()
  }
}

module.exports = { activate, deactivate }

const fs = require('node:fs/promises')
const path = require('node:path')
const vscode = require('vscode')

const PREVIEW_VIEW_TYPE = 'taffyCanvas.preview'
const FILE_PATTERN = '**/*.taffy.xml'

async function activate(context) {
  let panel = null

  await ensureXmlSchemaAssociation(context)

  const command = vscode.commands.registerCommand('taffyCanvas.openPreview', async () => {
    const editor = vscode.window.activeTextEditor
    if (!editor) {
      vscode.window.showErrorMessage('Open a Taffy Canvas XML file before starting the preview.')
      return
    }

    if (!panel) {
      panel = createPreviewPanel(context)
      panel.onDidDispose(() => {
        panel = null
      })
    }

    panel.reveal(vscode.ViewColumn.Beside)
    await renderPreview(panel, editor.document)
  })

  const onChange = vscode.workspace.onDidChangeTextDocument(async (event) => {
    if (!panel || !vscode.workspace.getConfiguration('taffyCanvas').get('preview.autoRefresh', true)) {
      return
    }
    const active = vscode.window.activeTextEditor?.document
    if (active && event.document.uri.toString() === active.uri.toString()) {
      await renderPreview(panel, event.document)
    }
  })

  const onSwitch = vscode.window.onDidChangeActiveTextEditor(async (editor) => {
    if (!panel || !editor) {
      return
    }
    await renderPreview(panel, editor.document)
  })

  context.subscriptions.push(command, onChange, onSwitch)
}

function createPreviewPanel(context) {
  const extensionUri = vscode.Uri.file(__dirname)
  const workspaceFolder = vscode.workspace.workspaceFolders?.[0]?.uri
  const runtimeUri = vscode.Uri.joinPath(
    extensionUri,
    'web',
    'index.js'
  )
  const moduleUri = vscode.Uri.joinPath(
    extensionUri,
    'web',
    'dist',
    'taffy_canvas_wasm.js'
  )
  const controllerUri = vscode.Uri.joinPath(extensionUri, 'media', 'preview.js')
  const roots = [extensionUri]
  if (workspaceFolder) {
    roots.push(workspaceFolder)
  }

  const panel = vscode.window.createWebviewPanel(
    PREVIEW_VIEW_TYPE,
    'Taffy Canvas Preview',
    vscode.ViewColumn.Beside,
    {
      enableScripts: true,
      localResourceRoots: roots,
      retainContextWhenHidden: true
    }
  )

  panel.webview.html = getWebviewHtml(
    panel.webview,
    panel.webview.asWebviewUri(runtimeUri),
    panel.webview.asWebviewUri(moduleUri),
    panel.webview.asWebviewUri(controllerUri)
  )
  return panel
}

async function renderPreview(panel, document) {
  const webview = panel.webview
  const title = document.isUntitled ? 'Taffy Canvas Preview' : `Taffy Canvas Preview: ${path.basename(document.uri.fsPath)}`
  panel.title = title

  const resources = await loadResourcesForDocument(webview, document)
  const params = vscode.workspace.getConfiguration('taffyCanvas').get('preview.params', {})

  webview.postMessage({
    type: 'render',
    payload: {
      xml: document.getText(),
      title,
      resources,
      params
    }
  })
}

async function loadResourcesForDocument(webview, document) {
  const manifestUri = await resolveManifestUri(document)
  if (!manifestUri) {
    return { assets: {}, fonts: {} }
  }

  try {
    const raw = await fs.readFile(manifestUri.fsPath, 'utf8')
    const manifest = JSON.parse(raw)
    const baseDir = path.dirname(manifestUri.fsPath)
    return {
      assets: resolveManifestMap(webview, baseDir, manifest.assets),
      fonts: resolveManifestMap(webview, baseDir, manifest.fonts)
    }
  } catch (error) {
    vscode.window.showWarningMessage(`Taffy Canvas preview could not read ${manifestUri.fsPath}: ${error}`)
    return { assets: {}, fonts: {} }
  }
}

async function resolveManifestUri(document) {
  const configured = vscode.workspace.getConfiguration('taffyCanvas').get('preview.resourceManifest', '')
  if (configured) {
    const workspaceFolder = vscode.workspace.getWorkspaceFolder(document.uri)
    const base = workspaceFolder?.uri.fsPath || path.dirname(document.uri.fsPath)
    const resolved = path.isAbsolute(configured) ? configured : path.resolve(base, configured)
    if (await exists(resolved)) {
      return vscode.Uri.file(resolved)
    }
  }

  if (document.isUntitled) {
    return null
  }

  const candidates = [
    path.join(path.dirname(document.uri.fsPath), 'resources.json'),
    path.join(path.dirname(document.uri.fsPath), 'taffy.resources.json')
  ]

  for (const candidate of candidates) {
    if (await exists(candidate)) {
      return vscode.Uri.file(candidate)
    }
  }

  return null
}

function resolveManifestMap(webview, baseDir, entries) {
  if (!entries || typeof entries !== 'object') {
    return {}
  }

  return Object.fromEntries(
    Object.entries(entries)
      .filter(([, value]) => typeof value === 'string')
      .map(([key, relativePath]) => {
        const absolutePath = path.resolve(baseDir, relativePath)
        return [key, webview.asWebviewUri(vscode.Uri.file(absolutePath)).toString()]
      })
  )
}

async function exists(filePath) {
  try {
    await fs.access(filePath)
    return true
  } catch {
    return false
  }
}

function getWebviewHtml(webview, runtimeUri, moduleUri, controllerUri) {
  const nonce = String(Date.now())
  const csp = [
    "default-src 'none'",
    `img-src ${webview.cspSource} https: data:`,
    `font-src ${webview.cspSource} https: data:`,
    `style-src ${webview.cspSource} 'unsafe-inline'`,
    `script-src 'nonce-${nonce}' 'wasm-unsafe-eval' 'unsafe-eval'`,
    `connect-src ${webview.cspSource} https: data:`
  ].join('; ')

  return `<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta http-equiv="Content-Security-Policy" content="${csp}" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <meta name="taffy-runtime" content="${runtimeUri}" />
    <meta name="taffy-module" content="${moduleUri}" />
    <style>
      :root {
        color-scheme: light dark;
      }
      html, body {
        height: 100%;
        overflow: hidden;
      }
      body {
        margin: 0;
        font-family: var(--vscode-font-family, system-ui, sans-serif);
        background: var(--vscode-editor-background);
        color: var(--vscode-editor-foreground);
      }
      .shell {
        position: relative;
        height: 100vh;
      }
      .controls {
        position: absolute;
        top: 0;
        right: 0;
        z-index: 1;
        display: flex;
        justify-content: flex-end;
        gap: 6px;
        padding: 10px;
        pointer-events: none;
      }
      .controls-inner {
        display: inline-flex;
        align-items: center;
        gap: 4px;
        padding: 4px;
        border: 1px solid var(--vscode-widget-border, var(--vscode-panel-border, transparent));
        border-radius: 6px;
        background: var(--vscode-editorWidget-background, var(--vscode-sideBar-background, var(--vscode-editor-background)));
        box-shadow: 0 2px 10px rgba(0, 0, 0, 0.16);
        pointer-events: auto;
      }
      .controls select {
        max-width: 220px;
        height: 28px;
        padding: 0 28px 0 8px;
        border: 0;
        border-radius: 4px;
        background: var(--vscode-dropdown-background, var(--vscode-input-background));
        color: var(--vscode-dropdown-foreground, var(--vscode-input-foreground));
        font: inherit;
        appearance: none;
        -webkit-appearance: none;
      }
      .controls button {
        min-width: 28px;
        height: 28px;
        margin: 0;
        border: 0;
        border-radius: 4px;
        background: transparent;
        color: var(--vscode-foreground);
        font: inherit;
        cursor: pointer;
      }
      .controls button:hover {
        background: var(--vscode-toolbar-hoverBackground, rgba(127, 127, 127, 0.16));
      }
      .controls button:active {
        background: var(--vscode-toolbar-activeBackground, rgba(127, 127, 127, 0.24));
      }
      .controls button:disabled {
        opacity: 0.5;
        cursor: default;
      }
      .zoom-label {
        min-width: 38px;
        padding: 0 2px;
        text-align: center;
        font-size: 12px;
        color: var(--vscode-descriptionForeground, var(--vscode-editor-foreground));
      }
      #error {
        display: none;
        position: absolute;
        left: 12px;
        right: 12px;
        top: 56px;
        margin: 0;
        padding: 10px 12px;
        border: 1px solid var(--vscode-inputValidation-errorBorder, transparent);
        background: var(--vscode-inputValidation-errorBackground, transparent);
        color: var(--vscode-inputValidation-errorForeground, var(--vscode-editor-foreground));
        border-radius: 4px;
        white-space: pre-wrap;
      }
      #viewport {
        display: grid;
        place-items: center;
        width: 100%;
        height: 100%;
        padding: 16px;
        overflow: auto;
        box-sizing: border-box;
      }
      #mount {
        display: flex;
        align-items: center;
        justify-content: center;
        min-width: 100%;
        min-height: 100%;
      }
      #mount img {
        display: block;
        border: 1px solid var(--vscode-panel-border, var(--vscode-editorGroup-border));
        background: var(--vscode-editor-background);
        cursor: zoom-in;
      }
    </style>
  </head>
  <body>
    <div class="shell">
      <div class="controls">
        <div class="controls-inner">
          <select id="preview-preset" title="Preview Preset" hidden></select>
          <button id="zoom-out" type="button" title="Zoom Out">-</button>
          <div id="zoom-label" class="zoom-label">Fit</div>
          <button id="zoom-in" type="button" title="Zoom In">+</button>
          <button id="zoom-reset" type="button" title="Actual Size">100%</button>
          <button id="zoom-fit" type="button" title="Fit to Window">Fit</button>
        </div>
      </div>
      <pre id="error"></pre>
      <div id="viewport">
        <div id="mount"></div>
      </div>
      </div>
    <script nonce="${nonce}" type="module" src="${controllerUri}"></script>
  </body>
</html>`
}

async function ensureXmlSchemaAssociation(context) {
  const schemaPath = path.join(context.extensionPath, 'schemas', 'taffy-canvas.xsd')

  if (!(await exists(schemaPath))) {
    return
  }

  const xmlConfig = vscode.workspace.getConfiguration('xml')
  const associations = normalizeAssociations(xmlConfig.get('fileAssociations'))
  const next = upsertAssociation(associations, {
    pattern: FILE_PATTERN,
    systemId: schemaPath
  })

  if (!next.changed) {
    return
  }

  const target = hasWorkspaceContext()
    ? vscode.ConfigurationTarget.Workspace
    : vscode.ConfigurationTarget.Global

  await xmlConfig.update('fileAssociations', next.value, target)
}

function hasWorkspaceContext() {
  return Boolean(vscode.workspace.workspaceFile || vscode.workspace.workspaceFolders?.length)
}

function normalizeAssociations(value) {
  return Array.isArray(value) ? value.filter((item) => item && typeof item === 'object') : []
}

function upsertAssociation(associations, desired) {
  const next = [...associations]
  const index = next.findIndex((entry) => entry.pattern === desired.pattern)

  if (index === -1) {
    next.push(desired)
    return { changed: true, value: next }
  }

  const current = next[index]
  if (current.systemId === desired.systemId) {
    return { changed: false, value: associations }
  }

  if (isManagedTaffyAssociation(current)) {
    next[index] = desired
    return { changed: true, value: next }
  }

  return { changed: false, value: associations }
}

function isManagedTaffyAssociation(entry) {
  return typeof entry.systemId === 'string'
    && (entry.systemId.includes('taffy-canvas-preview') || entry.systemId.endsWith('taffy-canvas.xsd'))
}

function deactivate() {}

module.exports = {
  activate,
  deactivate
}

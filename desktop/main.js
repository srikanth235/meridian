// Electron shell for Meridian.
//
// On launch:
//   1. Pick a free localhost port.
//   2. Spawn the bundled `meridian` daemon on it.
//   3. Wait for /api/health to come up.
//   4. Open a BrowserWindow pointed at the daemon's React UI.
//
// Remote debugging port (default 9333) is enabled so Chrome DevTools and
// Claude's CDP-based inspection can attach to the renderer.

const { app, BrowserWindow, Menu, shell, dialog } = require('electron');
const { spawn } = require('node:child_process');
const net = require('node:net');
const path = require('node:path');
const fs = require('node:fs');
const os = require('node:os');
const http = require('node:http');

const REMOTE_DEBUG_PORT = process.env.MERIDIAN_REMOTE_DEBUG_PORT || '9444';
app.commandLine.appendSwitch('remote-debugging-port', REMOTE_DEBUG_PORT);
app.commandLine.appendSwitch('remote-allow-origins', '*');

const REPO_ROOT = path.resolve(__dirname, '..');
let daemonProc = null;
let mainWindow = null;

// ----- daemon binary + workflow resolution ---------------------------------

function resolveDaemonBinary() {
  if (process.env.MERIDIAN_BIN && fs.existsSync(process.env.MERIDIAN_BIN)) {
    return process.env.MERIDIAN_BIN;
  }
  if (app.isPackaged) {
    // Bundled inside the .app's Resources/bin/meridian (electron-builder
    // extraResource).
    return path.join(process.resourcesPath, 'bin', 'meridian');
  }
  // Dev: prefer release, fall back to debug.
  const candidates = [
    path.join(REPO_ROOT, 'target', 'release', 'meridian'),
    path.join(REPO_ROOT, 'target', 'debug', 'meridian'),
  ];
  for (const c of candidates) {
    if (fs.existsSync(c)) return c;
  }
  throw new Error(
    `meridian binary not found. Run \`cargo build --release\` first, or set MERIDIAN_BIN.`
  );
}

function resolveWorkflowPath() {
  if (process.env.MERIDIAN_WORKFLOW && fs.existsSync(process.env.MERIDIAN_WORKFLOW)) {
    return process.env.MERIDIAN_WORKFLOW;
  }
  if (!app.isPackaged) {
    // Dev: use the repo's WORKFLOW.md.
    return path.join(REPO_ROOT, 'WORKFLOW.md');
  }
  // Packaged: look in user-config dir, scaffold from bundled template if missing.
  const userDir = path.join(
    os.homedir(),
    'Library',
    'Application Support',
    'Meridian'
  );
  fs.mkdirSync(userDir, { recursive: true });
  const userPath = path.join(userDir, 'WORKFLOW.md');
  if (!fs.existsSync(userPath)) {
    const bundled = path.join(process.resourcesPath, 'WORKFLOW.md');
    if (fs.existsSync(bundled)) {
      fs.copyFileSync(bundled, userPath);
    } else {
      fs.writeFileSync(userPath, '# Meridian workflow — please configure\n');
    }
  }
  return userPath;
}

// ----- helpers --------------------------------------------------------------

function findFreePort() {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.unref();
    srv.on('error', reject);
    srv.listen(0, '127.0.0.1', () => {
      const port = srv.address().port;
      srv.close(() => resolve(port));
    });
  });
}

function waitForHealth(port, timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const tick = () => {
      const req = http.get(
        { host: '127.0.0.1', port, path: '/api/health', timeout: 500 },
        (res) => {
          res.resume();
          if (res.statusCode === 200) return resolve();
          retry();
        }
      );
      req.on('error', retry);
      req.on('timeout', () => {
        req.destroy();
        retry();
      });
    };
    const retry = () => {
      if (Date.now() > deadline) {
        return reject(new Error(`daemon /api/health did not respond within ${timeoutMs}ms`));
      }
      setTimeout(tick, 200);
    };
    tick();
  });
}

// ----- daemon process -------------------------------------------------------

function spawnDaemon(port, workflowPath) {
  const binary = resolveDaemonBinary();
  console.log(`[meridian] spawning ${binary} --port ${port} --workflow ${workflowPath}`);
  const child = spawn(
    binary,
    ['--workflow', workflowPath, '--port', String(port), '--host', '127.0.0.1'],
    {
      env: { ...process.env, RUST_LOG: process.env.RUST_LOG || 'info' },
      stdio: ['ignore', 'pipe', 'pipe'],
    }
  );
  child.stdout.on('data', (d) => process.stdout.write(`[meridian] ${d}`));
  child.stderr.on('data', (d) => process.stderr.write(`[meridian] ${d}`));
  child.on('exit', (code, signal) => {
    console.log(`[meridian] daemon exited code=${code} signal=${signal}`);
    daemonProc = null;
    if (!app.isQuitting) {
      // If the daemon dies unexpectedly, surface it before the renderer
      // shows a blank page.
      dialog.showErrorBox(
        'Meridian daemon stopped',
        `The backend exited (code=${code}, signal=${signal}). Check the console for details.`
      );
      app.quit();
    }
  });
  return child;
}

function killDaemon() {
  if (!daemonProc || daemonProc.killed) return;
  console.log(`[meridian] stopping daemon pid=${daemonProc.pid}`);
  try {
    daemonProc.kill('SIGTERM');
  } catch (e) {
    console.warn('[meridian] kill failed:', e);
  }
}

// ----- window ---------------------------------------------------------------

function createWindow(daemonPort) {
  mainWindow = new BrowserWindow({
    width: 1400,
    height: 900,
    minWidth: 900,
    minHeight: 600,
    backgroundColor: '#0b0d10',
    titleBarStyle: 'hiddenInset',
    title: 'Meridian',
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      preload: path.join(__dirname, 'preload.js'),
    },
  });

  // Open external links in the user's default browser instead of inside the app.
  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith('http://') || url.startsWith('https://')) {
      shell.openExternal(url);
      return { action: 'deny' };
    }
    return { action: 'allow' };
  });

  mainWindow.loadURL(`http://127.0.0.1:${daemonPort}/`);
  mainWindow.on('closed', () => {
    mainWindow = null;
  });
}

function buildMenu() {
  const template = [
    ...(process.platform === 'darwin'
      ? [
          {
            label: app.name,
            submenu: [
              { role: 'about' },
              { type: 'separator' },
              { role: 'hide' },
              { role: 'hideOthers' },
              { role: 'unhide' },
              { type: 'separator' },
              { role: 'quit' },
            ],
          },
        ]
      : []),
    {
      label: 'View',
      submenu: [
        { role: 'reload' },
        { role: 'forceReload' },
        { role: 'toggleDevTools' },
        { type: 'separator' },
        { role: 'resetZoom' },
        { role: 'zoomIn' },
        { role: 'zoomOut' },
        { type: 'separator' },
        { role: 'togglefullscreen' },
      ],
    },
    {
      label: 'Window',
      submenu: [{ role: 'minimize' }, { role: 'close' }],
    },
  ];
  Menu.setApplicationMenu(Menu.buildFromTemplate(template));
}

// ----- lifecycle ------------------------------------------------------------

async function startup() {
  let port, workflow;
  try {
    port = await findFreePort();
    workflow = resolveWorkflowPath();
    daemonProc = spawnDaemon(port, workflow);
    await waitForHealth(port);
  } catch (e) {
    console.error('[meridian] startup failed:', e);
    dialog.showErrorBox('Meridian failed to start', String(e.message || e));
    app.quit();
    return;
  }
  buildMenu();
  createWindow(port);
}

app.on('before-quit', () => {
  app.isQuitting = true;
  killDaemon();
});
app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
app.on('activate', () => {
  if (BrowserWindow.getAllWindows().length === 0) startup();
});

app.whenReady().then(startup);

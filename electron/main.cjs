const { app, BrowserWindow, ipcMain } = require('electron')
const { execFile } = require('node:child_process')
const path = require('node:path')

function createWindow() {
  const win = new BrowserWindow({ width: 1440, height: 920, minWidth: 1000, minHeight: 680, backgroundColor: '#101214', webPreferences: { preload: path.join(__dirname, 'preload.cjs'), contextIsolation: true, nodeIntegration: false } })
  if (process.env.VITE_DEV_SERVER_URL) win.loadURL(process.env.VITE_DEV_SERVER_URL)
  else win.loadFile(path.join(__dirname, '../dist/index.html'))
}
app.whenReady().then(() => {
  ipcMain.handle('kodework:ssh', (_, { host, args = [] }) => new Promise((resolve) => {
    execFile('ssh', [host, ...args], { windowsHide: true }, (error, stdout, stderr) => resolve({ ok: !error, stdout, stderr: stderr || error?.message || '' }))
  }))
  ipcMain.handle('kodework:tailscale-status', () => new Promise((resolve) => {
    execFile('tailscale', ['status', '--json'], { windowsHide: true }, (error, stdout, stderr) => resolve({ ok: !error, stdout, stderr: stderr || error?.message || '' }))
  }))
  createWindow()
  app.on('activate', () => { if (BrowserWindow.getAllWindows().length === 0) createWindow() })
})
app.on('window-all-closed', () => { if (process.platform !== 'darwin') app.quit() })

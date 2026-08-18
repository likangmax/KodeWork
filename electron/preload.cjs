const { contextBridge, ipcRenderer } = require('electron')
contextBridge.exposeInMainWorld('kodework', {
  ssh: (host, args) => ipcRenderer.invoke('kodework:ssh', { host, args }),
  tailscaleStatus: () => ipcRenderer.invoke('kodework:tailscale-status'),
})

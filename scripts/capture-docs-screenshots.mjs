import { chromium } from 'playwright'
import fs from 'node:fs/promises'

const outputDir = 'docs/assets/readme'
await fs.mkdir(outputDir, { recursive: true })

const browser = await chromium.launch({ headless: true })
const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 })

await page.addInitScript(() => {
  localStorage.setItem('kodework.language.v1', 'en-US')

  const callbacks = new Map()
  let nextCallbackId = 1
  const encoder = new TextEncoder()
  const host = {
    id: '11111111-1111-4111-8111-111111111111',
    label: 'Demo Workstation',
    username: 'testuser',
    port: 22,
    auth_ref: { provider: 'docs', opaque_id: 'ssh-password/demo-workstation' },
    auth_mode: 'Password',
    private_key_path: null,
    default_remote_path: '/home/testuser/project',
    jump: {
      hostname: '198.51.100.20',
      port: 22,
      username: 'jumpuser',
      auth_ref: null,
      auth_mode: 'SshAgent',
      private_key_path: null,
    },
    addresses: [{
      id: '22222222-2222-4222-8222-222222222222',
      kind: 'Manual',
      hostname_or_ip: '192.0.2.10',
      port: 22,
      priority: 10,
      enabled: true,
    }],
    tailscale: {
      enabled: true,
      mode: 'SystemDaemon',
      device_name: 'demo-workstation',
      auth_key_ref: null,
      state_dir: null,
    },
    default_runtime: 'Tmux',
  }

  const emit = (channel, payload) => {
    if (!channel) return
    try {
      if (typeof channel.onmessage === 'function') {
        channel.onmessage(payload)
        return
      }
    } catch {}
    const id = channel.id ?? channel.__TAURI_CHANNEL_ID__ ?? channel
    if (callbacks.has(id)) callbacks.get(id)(payload)
  }

  const terminalText = [
    'Welcome to KodeWork documentation demo',
    'Synthetic workstation · no private infrastructure',
    '',
    'testuser@demo-workstation:/home/testuser/project$ git status',
    'On branch demo',
    'nothing to commit, working tree clean',
    'testuser@demo-workstation:/home/testuser/project$ cargo test -q',
    'test result: ok. 24 passed; 0 failed',
    'testuser@demo-workstation:/home/testuser/project$ ',
  ].join('\r\n')

  Object.defineProperty(window, '__TAURI_INTERNALS__', {
    configurable: true,
    value: {
      transformCallback(callback, once = false) {
        const id = nextCallbackId++
        callbacks.set(id, (...args) => {
          callback(...args)
          if (once) callbacks.delete(id)
        })
        return id
      },
      unregisterCallback(id) {
        callbacks.delete(id)
      },
      async invoke(command, args = {}) {
        switch (command) {
          case 'list_hosts': return [host]
          case 'prepare_host_network': return null
          case 'connect_host': return 'Connected to Demo Workstation'
          case 'disconnect_host': return null
          case 'session_state': return 'Ready'
          case 'session_runtime_subscribe':
            setTimeout(() => emit(args.onEvent, { state: 'Ready', generation: 1 }), 80)
            return null
          case 'open_pane': return [1, 1]
          case 'close_pane':
          case 'send_input':
          case 'resize_pty':
          case 'clipboard_copy_text': return null
          case 'session_subscribe':
            setTimeout(() => emit(args.onEvent, {
              Data: { channel: 1, bytes: Array.from(encoder.encode(terminalText)) },
            }), 180)
            return null
          case 'clipboard_paste': return { kind: 'empty' }
          case 'local_terminal_capabilities': return {
            powershell: true,
            command_prompt: true,
            wsl: true,
            wsl_distributions: ['Ubuntu-24.04'],
          }
          case 'snippet_list': return []
          case 'project_list': return []
          case 'action_list': return []
          case 'run_reconcile': return 0
          case 'run_list': return []
          case 'tunnel_list': return []
          case 'tmux_list': return [{ name: 'demo', windows: 1, attached: 1, created: 'synthetic' }]
          case 'herdr_detect': return '0.5.0-docs'
          case 'herdr_agents': return []
          case 'tailscale_runtime_info': return {
            cli_available: true,
            daemon_available: true,
            bundled: true,
            bundled_version: '1.90.6',
          }
          case 'autostart_status': return false
          case 'sftp_list': return [
            { name: 'src', size: 0, is_dir: true, modified_ms: 1789488000000 },
            { name: 'docs', size: 0, is_dir: true, modified_ms: 1789488000000 },
            { name: 'README.md', size: 4210, is_dir: false, modified_ms: 1789488000000 },
            { name: 'Cargo.toml', size: 986, is_dir: false, modified_ms: 1789488000000 },
            { name: 'package.json', size: 1384, is_dir: false, modified_ms: 1789488000000 },
            { name: 'demo-notes.txt', size: 768, is_dir: false, modified_ms: 1789488000000 },
          ]
          case 'sftp_subscribe':
            setTimeout(() => emit(args.onEvent, {
              Progress: {
                id: 'synthetic-transfer',
                progress: { transferred: 7340032, total: 10485760, speed_bps: 1572864 },
              },
            }), 180)
            setTimeout(() => emit(args.onEvent, {
              State: { id: 'synthetic-transfer', status: 'Transferring' },
            }), 220)
            return null
          case 'save_host': return null
          default:
            if (command.endsWith('_list')) return []
            return null
        }
      },
    },
  })
})

page.on('console', message => console.log(`[browser:${message.type()}] ${message.text()}`))
page.on('pageerror', error => console.error(`[browser:pageerror] ${error.stack || error.message}`))

await page.goto('http://127.0.0.1:4173', { waitUntil: 'networkidle' })
await page.getByText('Demo Workstation', { exact: true }).first().waitFor({ timeout: 10000 })

await page.getByRole('button', { name: 'Connect', exact: true }).click()
await page.getByText('Connected', { exact: true }).first().waitFor({ timeout: 10000 })
await page.locator('.terminal-host').waitFor({ timeout: 10000 })
await page.waitForTimeout(900)
await page.screenshot({ path: `${outputDir}/workspace-terminal.png`, fullPage: false })

await page.getByRole('button', { name: 'Files', exact: true }).click()
await page.locator('.files-card').waitFor({ timeout: 10000 })
await page.getByText('README.md', { exact: true }).waitFor({ timeout: 10000 })
await page.waitForTimeout(500)
await page.screenshot({ path: `${outputDir}/files-transfers.png`, fullPage: false })

await page.getByRole('button', { name: 'Edit', exact: true }).click()
await page.getByRole('heading', { name: 'Edit workstation', exact: true }).waitFor({ timeout: 10000 })
await page.waitForTimeout(350)
const modal = page.locator('.host-modal.host-editor')
await modal.screenshot({ path: `${outputDir}/workstation-config.png` })

await browser.close()
console.log('Captured sanitized KodeWork documentation screenshots.')

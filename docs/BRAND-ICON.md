# KodeWork application icon

## Design language

- Dark graphite rounded background: Windows development workbench and terminal environment.
- Coral-orange outline: combines a terminal window, remote connection path, and the original KodeWork visual identity.
- `>_`: SSH/PTY command workflow.
- Mint-green node: remote-node availability and Tailscale/Herdr connection state.

The icon avoids text, fine lines, and complex textures so the silhouette remains legible at 16–32 px in the taskbar, tray, shortcuts, and installer surfaces.

## Assets

- Source artwork: `assets/branding/kodework-icon-master.png`
- Windows ICO: `src-tauri/icons/icon.ico`
- Tauri PNG assets: `32x32.png`, `64x64.png`, `128x128.png`, `128x128@2x.png`
- Windows Store and additional platform sizes are generated from the source artwork.

## Regenerating platform icons

From the repository root, regenerate the Tauri icon set from the committed source artwork:

```powershell
npx tauri icon assets/branding/kodework-icon-master.png
```

Review generated binary diffs before committing them and verify the Windows taskbar, tray, shortcut, and installer rendering at their native sizes. Do not replace the source artwork as part of an unrelated code or documentation change.

# KodeWork Cross-Platform Roadmap

## Vision

KodeWork aims to be a universal SSH workspace manager available on all major platforms: **Windows, macOS, Linux, iOS, and Android**. Users should have a consistent, high-quality experience regardless of their device.

## Current Status (v0.2.3)

- ✅ **Windows 10/11 x64**: Production-ready with full feature set
- ⚠️ **macOS**: Partial — Tauri desktop foundation exists, but:
  - Credential storage needs macOS Keychain integration
  - Local terminal needs PTY adapter for Darwin
  - Build/release pipeline not yet configured
- ⚠️ **Linux**: Partial — Tauri supports Linux, but:
  - Credential storage needs Secret Service / libsecret integration
  - Package formats (deb, rpm, AppImage) not configured
  - Desktop integration and system tray needs testing
- ❌ **iOS**: Not started
- ❌ **Android**: Not started

## Platform Architecture Strategy

### Desktop Platforms (Windows, macOS, Linux)

**Current Foundation:** Tauri 2.x with Rust core + React UI

**Advantages:**
- Single Rust codebase for business logic
- Native performance
- Small binary size (~15 MB)
- Native OS integration

**Platform-Specific Work Required:**

#### macOS
1. **Credential Storage** (Priority 1)
   - Implement `kodework-secrets-mac` crate using macOS Keychain API
   - Match Windows `kodework-secrets-win` API surface
   - Estimated effort: 1-2 weeks

2. **Local PTY** (Priority 1)
   - Implement `kodework-local-pty-unix` using `libc::forkpty`
   - Support zsh, bash, iTerm2 integration
   - Estimated effort: 1 week

3. **Build Pipeline** (Priority 2)
   - Configure Tauri bundler for `.dmg` and `.app`
   - Notarization for App Store and direct distribution
   - GitHub Actions runner on macOS
   - Estimated effort: 3-5 days

4. **System Integration** (Priority 2)
   - Menu bar integration (native status bar item)
   - Dock icon and notifications
   - macOS specific keyboard shortcuts
   - Estimated effort: 3-5 days

#### Linux
1. **Credential Storage** (Priority 1)
   - Implement `kodework-secrets-linux` using `libsecret` (GNOME Keyring)
   - Fallback to encrypted file store for headless systems
   - Support for KWallet (KDE) as secondary option
   - Estimated effort: 1-2 weeks

2. **Local PTY** (Priority 1)
   - Reuse macOS `kodework-local-pty-unix` implementation
   - Test with bash, zsh, fish shells
   - Estimated effort: 2-3 days (mostly testing)

3. **Build Pipeline** (Priority 2)
   - `.deb` packages for Debian/Ubuntu
   - `.rpm` packages for Fedora/RHEL
   - AppImage for universal Linux compatibility
   - Flatpak for sandboxed distribution
   - Estimated effort: 1 week

4. **System Integration** (Priority 2)
   - System tray using `libappindicator` (Ubuntu) or `StatusNotifierItem` (KDE)
   - `.desktop` file and XDG autostart integration
   - Wayland and X11 compatibility testing
   - Estimated effort: 3-5 days

### Mobile Platforms (iOS, Android)

**Challenge:** Tauri does not support mobile. A different architecture is required.

**Strategic Options:**

#### Option A: React Native (Recommended)
- **Pros:**
  - Reuse existing React UI components (~80% code reuse)
  - Mature SSH libraries available (`react-native-ssh-sftp`, or native modules)
  - Native performance for UI
  - Large community and ecosystem
  - Single codebase for iOS and Android

- **Cons:**
  - Cannot reuse Rust core directly (different FFI model)
  - Need to reimplement business logic in TypeScript or write native modules
  - Larger app size than Tauri desktop (~30-50 MB)

- **Architecture:**
  ```
  React Native UI (shared)
  ├── TypeScript business logic (ported from Rust core)
  ├── Native SSH module (C++/Objective-C for iOS, C++/Java for Android)
  ├── Secure storage (Keychain on iOS, Keystore on Android)
  └── SQLite (same schema as desktop)
  ```

- **Estimated Effort:** 3-4 months for initial release

#### Option B: Flutter
- **Pros:**
  - Excellent cross-platform UI consistency
  - Good SSH packages available (`dartssh2`)
  - Rust FFI possible via `flutter_rust_bridge`
  - Single codebase for mobile and potentially desktop

- **Cons:**
  - Complete UI rewrite (no React reuse)
  - Different programming model (Dart)
  - Larger app size (~30-50 MB)
  - Smaller community than React Native

- **Estimated Effort:** 4-5 months for initial release

#### Option C: Native (Swift + Kotlin)
- **Pros:**
  - Best native performance and OS integration
  - Direct access to platform APIs
  - Smallest possible app size

- **Cons:**
  - Two completely separate codebases
  - No code reuse with desktop
  - 2x development and maintenance effort
  - Slowest time to market

- **Estimated Effort:** 6-8 months for both platforms

**Recommendation: React Native (Option A)**
- Fastest path to mobile presence
- Maximal code reuse with desktop UI
- Proven technology for SSH clients (e.g., Termius uses React Native)
- Can incrementally port Rust business logic if needed

### Mobile-Specific Features

Both iOS and Android will require:

1. **Biometric Authentication**
   - Touch ID / Face ID (iOS)
   - Fingerprint / Face Unlock (Android)
   - For unlocking credential vault

2. **Background Session Persistence**
   - iOS background task limitations
   - Android foreground service for persistent connections
   - Reconnect on app resume

3. **Mobile-Optimized UI**
   - Touch-first interface (no hover states)
   - Bottom sheet modals
   - Swipe gestures for navigation
   - Larger tap targets (44pt minimum)
   - Responsive terminal font sizes

4. **Clipboard Integration**
   - Share extension for quick file transfers
   - Copy/paste between terminal and other apps

5. **Notifications**
   - Transfer completion alerts
   - Connection status changes
   - Action run completion

6. **Reduced Feature Set (v1)**
   - No local terminals (mobile shells are limited)
   - Simplified file browser (mobile storage model)
   - Focus on remote SSH sessions and SFTP

## Implementation Phases

### Phase 1: Complete Windows Hardening (Current - Q4 2025)
**Status:** 95% complete
- ✅ Core architecture solid
- ✅ SSH, SFTP, PTY working reliably
- ✅ Herdr/Tmux integration
- ⏳ Final polish and documentation
- ⏳ Public release readiness

**Timeline:** Complete by end of August 2025

### Phase 2: macOS Support (Q4 2025 - Q1 2026)
**Priority:** High — desktop developers need macOS
- Credential storage adapter
- Unix PTY implementation
- Build pipeline and notarization
- Testing on multiple macOS versions (12+)
- Beta release to community

**Timeline:** 3-4 weeks after Windows release
**Target:** October 2025

### Phase 3: Linux Support (Q1 2026)
**Priority:** High — open-source community expects Linux support
- Credential storage with multiple backend support
- Comprehensive distro testing (Ubuntu, Fedora, Arch, Debian)
- Package formats for major distros
- Desktop environment integration testing

**Timeline:** 4-6 weeks
**Target:** December 2025

### Phase 4: iOS Initial Release (Q1-Q2 2026)
**Priority:** Medium — mobile is strategic but requires significant work
- React Native foundation
- Core SSH functionality (connection, terminal, basic SFTP)
- iOS-specific credential storage
- App Store submission process

**Timeline:** 3-4 months
**Target:** March 2026

### Phase 5: Android Initial Release (Q2 2026)
**Priority:** Medium — follows iOS architecture
- Reuse React Native codebase from iOS
- Android-specific adaptations (storage, notifications, background)
- Google Play submission

**Timeline:** 2-3 months (after iOS)
**Target:** May 2026

### Phase 6: Mobile Feature Parity (Q2-Q3 2026)
- Advanced SFTP features (resume, multi-file)
- Run Actions on mobile
- Herdr session management
- Cross-device session sync (optional cloud backend)

**Timeline:** 2-3 months
**Target:** August 2026

## Technical Decisions

### Credential Sync (Cross-Device)
**Question:** Should credentials sync across devices?

**Options:**
1. **No Sync (Current)** — each device has independent credential vault
   - Pros: Simplest, most secure, no cloud backend needed
   - Cons: Manual credential entry on each device

2. **Optional Cloud Sync** — encrypted credential vault stored in user's cloud
   - Pros: Seamless experience, user controls backend (iCloud, Google Drive, Dropbox)
   - Cons: Complexity, sync conflicts, trust model

3. **Self-Hosted Sync** — user runs their own sync server
   - Pros: Complete user control, open-source
   - Cons: Most complex, barrier to entry

**Decision (Phase 1-5):** No sync — each device is independent. Revisit in Phase 6 based on user demand.

### UI Consistency vs. Native Feel
**Question:** Should mobile UI match desktop exactly?

**Decision:** Prioritize **native mobile patterns** over pixel-perfect desktop match:
- Desktop: Dense information, hover, right-click menus, keyboard shortcuts
- Mobile: Touch-first, bottom sheets, swipe gestures, simplified views
- Share design language (colors, typography, iconography)
- Accept that layouts will differ significantly

### Database Schema Compatibility
**Decision:** All platforms use the **same SQLite schema** (from `kodework-storage`):
- Enables future export/import across devices
- Simplifies testing and debugging
- Mobile ports can reference desktop implementation

### Feature Flags
**Decision:** Use compile-time feature flags for platform-specific capabilities:
```rust
#[cfg(feature = "local-terminal")]
// Desktop-only: local PTY

#[cfg(feature = "system-tray")]
// Desktop-only: system tray icon

#[cfg(target_os = "ios")]
// iOS-specific code

#[cfg(target_os = "android")]
// Android-specific code
```

## Success Metrics

### Phase 2 (macOS) — Success Criteria:
- [ ] 10+ beta testers using daily for 1 week
- [ ] No P0/P1 bugs reported in core SSH/SFTP
- [ ] Keychain integration working across reboots
- [ ] Build size < 20 MB
- [ ] First-launch to first-connection < 2 minutes

### Phase 3 (Linux) — Success Criteria:
- [ ] Working on Ubuntu, Fedora, Arch (minimum)
- [ ] Both GNOME and KDE tested
- [ ] Wayland and X11 confirmed working
- [ ] AppImage works on distros without native packages
- [ ] Community feedback positive (>80% of reported issues addressed)

### Phase 4 (iOS) — Success Criteria:
- [ ] App Store approval achieved
- [ ] Core workflows (connect, terminal, SFTP) working smoothly
- [ ] No crashes or data loss in 1 week of testing
- [ ] Touch interface feels native (no desktop UI patterns)
- [ ] Background reconnection working reliably

### Phase 5 (Android) — Success Criteria:
- [ ] Google Play approval achieved
- [ ] Working across Samsung, Pixel, OnePlus devices
- [ ] Battery drain acceptable (< 5% per hour with active connection)
- [ ] Foreground service keeps connection alive
- [ ] Material Design guidelines followed

## Open Questions

1. **Should we support ChromeOS?**
   - ChromeOS can run Linux apps (Crostini) — might work automatically
   - Native Chrome app would require different architecture
   - Decision: Test Linux build on ChromeOS; if it works well, document it. No dedicated port initially.

2. **Should we support iPad-specific features?**
   - Split-view multitasking
   - External keyboard shortcuts
   - Mouse/trackpad support
   - Decision: Yes, but in Phase 6 (after core iOS release)

3. **Should we have a web version?**
   - Browser-based SSH client (no installation)
   - Use WebAssembly for Rust core
   - Limitations: browser security model restricts local file access
   - Decision: Not initially — native apps provide better experience and security. Revisit if user demand emerges.

4. **How to handle platform-specific bugs?**
   - Use GitHub issue labels: `platform:windows`, `platform:macos`, `platform:linux`, `platform:ios`, `platform:android`
   - Prioritize bugs that affect multiple platforms
   - Each platform has a designated testing lead

## Community and Contributions

As we expand to multiple platforms, we need:
- **Platform maintainers**: Contributors who own testing and bug triage for specific platforms
- **Beta testing programs**: Structured feedback collection per platform
- **Platform-specific documentation**: Installation, troubleshooting per OS
- **CI/CD per platform**: Automated builds and tests

**Call for contributors**: We welcome platform maintainers for macOS and Linux. See [CONTRIBUTING.md](../CONTRIBUTING.md).

## Conclusion

KodeWork's cross-platform journey is ambitious but achievable:
1. **Windows** is production-ready (Q4 2025)
2. **macOS** and **Linux** follow quickly with shared Rust core (Q4 2025 - Q1 2026)
3. **iOS** and **Android** require new architecture but leverage React ecosystem (Q1-Q2 2026)

By mid-2026, KodeWork will be a truly universal SSH workspace manager, providing a premium experience on every platform users work on.

**Status:** This roadmap will be updated quarterly as we make progress.

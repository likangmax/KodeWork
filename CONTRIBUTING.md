# Contributing to KodeWork

Thank you for your interest in contributing to KodeWork! This guide will help you understand our development process and how to submit quality contributions.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Pull Request Process](#pull-request-process)
- [Coding Standards](#coding-standards)
- [Testing Requirements](#testing-requirements)
- [Security Considerations](#security-considerations)
- [Documentation Guidelines](#documentation-guidelines)

## Code of Conduct

We are committed to providing a welcoming and inclusive environment. Please be respectful, constructive, and professional in all interactions.

## Getting Started

### Prerequisites

- **Windows 10/11 x64** (primary development platform)
- **Rust 1.98.0+** with MSVC toolchain (see `rust-toolchain.toml`)
- **Node.js 20+** and npm
- **Git** for version control
- **Visual Studio Build Tools** (for Rust MSVC toolchain)

### First-Time Setup

1. **Clone the repository:**
   ```bash
   git clone https://github.com/likangmax/KodeWork.git
   cd KodeWork
   ```

2. **Install dependencies:**
   ```bash
   npm ci
   cargo build
   ```

3. **Verify your environment:**
   ```bash
   npm run test:frontend
   cargo test --workspace --all-features
   npm run lint
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   ```

4. **Run the development build:**
   ```bash
   npm run desktop
   ```

### Understanding the Architecture

Read these documents before making changes:

1. [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) - System boundaries and data flows
2. [`docs/HANDOFF-CLAUDE-CODE.zh-CN.md`](docs/HANDOFF-CLAUDE-CODE.zh-CN.md) - Detailed implementation guide
3. [`docs/STATUS.md`](docs/STATUS.md) - Current project status and gaps
4. [`docs/adr/`](docs/adr/) - Architecture decision records

**Key architectural principles:**
- Rust owns connection truth, credentials, and lifecycle
- React owns presentation and renderer lifecycle only
- One-way dependency: UI → Tauri → Core → Domain → Adapters
- Bounded channels for high-frequency data (terminal, transfers)
- Fail-closed security boundaries (host keys, credentials)

## Development Workflow

### Branch Strategy

- `main` - stable release branch
- `feature/*` - new features
- `fix/*` - bug fixes
- `docs/*` - documentation improvements
- `refactor/*` - code improvements without behavior changes

### Making Changes

1. **Create a feature branch:**
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make atomic commits with clear messages:**
   ```bash
   git commit -m "feat: add SSH connection pooling"
   git commit -m "fix: prevent race in reconnect supervisor"
   git commit -m "docs: update ARCHITECTURE.md with new flow"
   ```

   Use conventional commit prefixes:
   - `feat:` - new feature
   - `fix:` - bug fix
   - `docs:` - documentation only
   - `test:` - add or update tests
   - `refactor:` - code change without behavior change
   - `perf:` - performance improvement
   - `chore:` - maintenance tasks
   - `security:` - security fix

3. **Keep commits focused:**
   - One logical change per commit
   - Don't mix refactoring with behavior changes
   - Don't mix formatting changes with logic changes

## Pull Request Process

### Before Submitting

1. **Ensure all tests pass:**
   ```powershell
   cargo fmt --all -- --check
   cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
   cargo test --locked --workspace --all-features
   npm run lint
   npm run test:frontend
   npm run build
   ```

2. **Run security audits:**
   ```powershell
   cargo audit --no-fetch
   npm audit --omit=dev --audit-level=high
   ```

3. **Check for secrets:**
   ```powershell
   git diff --check
   # Manually verify no credentials, keys, or sensitive data
   ```

4. **Update documentation:**
   - Add/update inline code documentation
   - Update relevant markdown files
   - Add tests for new functionality
   - Update CHANGELOG.md if applicable

### Submitting the PR

1. **Push your branch:**
   ```bash
   git push origin feature/your-feature-name
   ```

2. **Create a Pull Request with:**
   - **Clear title** describing the change
   - **Description** explaining:
     - What problem does this solve?
     - How does it solve it?
     - What was tested?
     - Any breaking changes?
     - Related issues (if any)

3. **PR Checklist:**
   - [ ] All tests pass locally
   - [ ] No new Clippy warnings
   - [ ] Code is formatted (rustfmt, prettier)
   - [ ] Documentation is updated
   - [ ] Security considerations addressed
   - [ ] No credentials or sensitive data committed
   - [ ] Commits are atomic and well-described

### Review Process

- Maintainers will review within 48 hours (typically)
- Address review feedback promptly
- Keep discussion focused and constructive
- Be open to alternative approaches

## Coding Standards

### Rust Code

1. **Follow Rust API Guidelines:**
   - Use `#[must_use]` for constructors and fallible operations
   - Prefer `Result` over panicking
   - Use `thiserror` for domain errors
   - Avoid `unwrap()` in production code

2. **Use explicit error types:**
   ```rust
   // Good
   fn connect(host: &str) -> Result<Connection, SshError>

   // Bad
   fn connect(host: &str) -> Result<Connection, Box<dyn Error>>
   ```

3. **Document public APIs:**
   ```rust
   /// Establishes an SSH connection to the specified host.
   ///
   /// # Arguments
   ///
   /// * `host` - Target hostname or IP address
   /// * `port` - SSH port (typically 22)
   ///
   /// # Errors
   ///
   /// Returns `SshError::Network` if the host is unreachable.
   /// Returns `SshError::Authentication` if credentials are invalid.
   ///
   /// # Example
   ///
   /// ```
   /// let conn = connect("example.com", 22)?;
   /// ```
   pub fn connect(host: &str, port: u16) -> Result<Connection, SshError>
   ```

4. **Prefer explicit lifetimes and ownership:**
   - Avoid unnecessary cloning
   - Use `Arc` for shared ownership, `Rc` sparingly
   - Document lifetime relationships

5. **Safety boundaries:**
   - Keep `#![forbid(unsafe_code)]` where possible
   - Document any `unsafe` with safety invariants
   - Use safe abstractions over raw FFI

### TypeScript/React Code

1. **Use TypeScript strictly:**
   ```typescript
   // Good
   interface Connection {
     hostId: string;
     state: ConnectionState;
   }

   // Bad - avoid 'any'
   function connect(opts: any): Promise<any>
   ```

2. **Component best practices:**
   - Keep components focused and small
   - Extract complex logic into hooks
   - Use `useCallback` and `useMemo` appropriately
   - Include all dependencies in hook arrays

3. **Naming conventions:**
   - `PascalCase` for components
   - `camelCase` for functions and variables
   - `UPPER_CASE` for constants
   - Prefix boolean props with `is`, `has`, `should`

### Project Structure

```
crates/
  kodework-domain/       # Core models, validation, no I/O
  kodework-core/         # Session orchestration, lifecycle
  kodework-ssh/          # SSH/PTY adapter
  kodework-sftp/         # SFTP streaming adapter
  kodework-storage/      # SQLite persistence
  kodework-secrets*/     # Credential management
  ...

src-tauri/               # Thin Tauri shell, IPC translation
src/                     # React UI, xterm.js terminals
docs/                    # Architecture, guides, evidence
scripts/                 # Build and verification helpers
```

## Testing Requirements

### Test Coverage Goals

- **Core domain logic**: 90%+ coverage
- **Connection handling**: Edge cases, timeouts, failures
- **SFTP transfers**: Resume, cancellation, source changes
- **Run lifecycle**: All state transitions, reconciliation
- **Security boundaries**: Host key verification, credentials

### Writing Good Tests

1. **Test behavior, not implementation:**
   ```rust
   #[test]
   fn reconnect_preserves_session_identity() {
       // Test observable behavior
   }
   ```

2. **Use descriptive test names:**
   ```rust
   #[test]
   fn host_key_store_error_blocks_connection()

   #[test]
   fn quick_timeout_after_dispatch_becomes_unknown()
   ```

3. **Avoid flaky tests:**
   - Use bounded timeouts
   - Don't depend on fixed sleep durations
   - Use fake clocks for time-dependent logic
   - Avoid filesystem races

4. **Test edge cases:**
   - Network failures mid-operation
   - Credential errors
   - Concurrent access
   - Large inputs
   - Empty inputs
   - Invalid states

## Security Considerations

### Critical Rules

1. **Never commit secrets:**
   - No passwords, private keys, API tokens
   - No real hostnames or infrastructure details
   - Use placeholder values in tests

2. **Fail closed on security boundaries:**
   - Host key store unavailable → block connection
   - Credential store unavailable → block connection
   - Trust verification failure → hard stop

3. **Credential handling:**
   - Credentials stay in OS secure storage
   - Use opaque references, never plaintext
   - Don't log credential material
   - Don't pass credentials through renderer

4. **Input validation:**
   - Validate remote paths before SFTP ops
   - Classify Action danger in Rust, not UI
   - Sanitize shell inputs
   - Bound all buffers and queues

5. **Audit trail:**
   - Document security-relevant changes
   - Add tests for security boundaries
   - Update SECURITY.md if changing threat model

### Reporting Security Issues

**Do not open public issues for security vulnerabilities.**

Report security issues privately through [SECURITY.md](SECURITY.md).

## Documentation Guidelines

### Code Documentation

1. **Document all public APIs:**
   - Purpose and behavior
   - Parameters and return values
   - Error conditions
   - Examples
   - Thread safety / concurrency considerations

2. **Module-level documentation:**
   ```rust
   //! # kodework-ssh
   //!
   //! SSH/PTY connection adapter with host key verification,
   //! multi-address fallback, and jump host support.
   //!
   //! ## Architecture
   //!
   //! ...
   ```

3. **Inline comments for non-obvious code:**
   - Why, not what
   - Explain invariants and assumptions
   - Reference related code or issues

### User Documentation

- **User guides**: Step-by-step workflows
- **Troubleshooting**: Common issues and solutions
- **Architecture docs**: High-level system design
- **ADRs**: Significant design decisions

### Keeping Docs Current

- Update docs in the same PR as code changes
- Flag outdated docs when you find them
- Verify code examples actually compile/run

## Questions?

- **General questions**: Open a GitHub Discussion
- **Bug reports**: Open an issue with repro steps
- **Security**: See [SECURITY.md](SECURITY.md)
- **Feature requests**: Open an issue describing the use case

Thank you for contributing to KodeWork!

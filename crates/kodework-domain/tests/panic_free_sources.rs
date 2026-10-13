//! Repo-wide guard for the "production code does not panic" property.
//!
//! The workspace denies `clippy::panic`, `clippy::unwrap_used` and
//! `clippy::expect_used`, but those lints do not cover `unreachable!`, `todo!`
//! or `unimplemented!`. `clippy::unreachable` is enabled on the library crates
//! (see each crate root) but deliberately not on `src-tauri`, because the
//! `#[tauri::command]` attribute expands to code containing `unreachable!()`
//! and the lint would report macro-generated panics we cannot remove. This test
//! covers that gap by reading the sources directly.
//!
//! Scope: `crates/*/src/**` and `src-tauri/src/**`. Integration tests under
//! `tests/` are excluded by design - they are expected to fail loudly.

use std::fs;
use std::path::{Path, PathBuf};

const BANNED: [&str; 3] = ["unreachable!(", "todo!(", "unimplemented!("];

fn collect_rust_sources(directory: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_rust_sources(&path, found)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
    Ok(())
}

/// The workspace convention is a single trailing `#[cfg(test)] mod tests`, so
/// everything from the first `#[cfg(test)]` line onwards is test code.
fn production_prefix(source: &str) -> &str {
    match source.find("#[cfg(test)]") {
        Some(index) => &source[..index],
        None => source,
    }
}

fn banned_macros_in(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (index, line) in production_prefix(source).lines().enumerate() {
        let trimmed = line.trim_start();
        // Comments may discuss these macros (this file does); they are not code.
        if trimmed.starts_with("//") {
            continue;
        }
        if let Some(name) = BANNED.iter().find(|name| line.contains(**name)) {
            found.push(format!("line {}: {} ({})", index + 1, trimmed, name));
        }
    }
    found
}

#[test]
fn production_sources_do_not_use_panicking_macros() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| unreachable!("domain crate lives at <root>/crates/kodework-domain"));

    let mut sources = Vec::new();
    let crates = root.join("crates");
    for entry in fs::read_dir(&crates).unwrap_or_else(|error| unreachable!("read crates/: {error}"))
    {
        let entry = entry.unwrap_or_else(|error| unreachable!("crates entry: {error}"));
        let src = entry.path().join("src");
        if src.is_dir() {
            collect_rust_sources(&src, &mut sources)
                .unwrap_or_else(|error| unreachable!("walk {}: {error}", src.display()));
        }
    }
    let tauri_src = root.join("src-tauri").join("src");
    collect_rust_sources(&tauri_src, &mut sources)
        .unwrap_or_else(|error| unreachable!("walk {}: {error}", tauri_src.display()));

    assert!(
        sources.len() > 20,
        "the scan must actually visit the workspace sources (found {})",
        sources.len()
    );

    let mut violations = Vec::new();
    for path in &sources {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|error| unreachable!("read {}: {error}", path.display()));
        for violation in banned_macros_in(&source) {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            violations.push(format!("{relative} {violation}"));
        }
    }

    assert!(
        violations.is_empty(),
        "production code must not panic; return an error instead:\n{}",
        violations.join("\n")
    );
}

#[test]
fn the_scan_detects_a_violation_before_the_test_module() {
    // Guards the guard: if this stops failing, the scan is broken rather than
    // the codebase being clean.
    let sample = "fn f() {\n    unreachable!(\"boom\");\n}\n\n#[cfg(test)]\nmod tests {\n    fn t() { todo!() }\n}\n";
    let found = banned_macros_in(sample);
    assert_eq!(
        found.len(),
        1,
        "only the production occurrence counts: {found:?}"
    );
    assert!(found[0].contains("unreachable!("));

    let all_test = "#[cfg(test)]\nmod tests {\n    fn t() { unreachable!() }\n}\n";
    assert!(banned_macros_in(all_test).is_empty());

    let comment_only = "// unreachable!() is explained here\nfn f() {}\n";
    assert!(banned_macros_in(comment_only).is_empty());
}

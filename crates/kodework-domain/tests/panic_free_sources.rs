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
//!
//! What this is and is not: it is a source-level convention check, not a proof
//! that no runtime panic exists (allocation failure, integer overflow in release
//! builds with overflow checks off, and third-party code are outside its reach).
//! It removes `#[cfg(test)]` items rather than truncating the file at the first
//! one, so production code that happens to follow a test module is still scanned.

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

/// Index just past the closing brace of the block that starts at `open`.
///
/// Braces inside line comments, block comments, string literals (including raw
/// strings) and character literals do not count. Returns `None` when the source
/// is malformed or unterminated, which callers treat as "cannot delimit, keep the
/// text" rather than as "no test code here".
fn block_end(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(open) != Some(&b'{') {
        return None;
    }
    let mut depth = 0usize;
    let mut index = open;
    while index < bytes.len() {
        match bytes[index] {
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                let mut nesting = 1usize;
                while index < bytes.len() && nesting > 0 {
                    if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
                        nesting += 1;
                        index += 2;
                    } else if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                        nesting -= 1;
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
            }
            b'"' => {
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index += 2,
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
            }
            b'\'' => {
                // A character literal, not a lifetime: `'x'` or `'\n'`.
                let literal = match (bytes.get(index + 1), bytes.get(index + 2)) {
                    (Some(b'\\'), Some(_)) => bytes.get(index + 3) == Some(&b'\''),
                    (Some(_), Some(&b'\'')) => true,
                    _ => false,
                };
                index += if literal { 4 } else { 1 };
            }
            b'r' if matches!(bytes.get(index + 1), Some(b'"') | Some(b'#')) => {
                // Raw string: r"..." or r#"..."# with any number of hashes.
                let mut hashes = 0usize;
                let mut probe = index + 1;
                while bytes.get(probe) == Some(&b'#') {
                    hashes += 1;
                    probe += 1;
                }
                if bytes.get(probe) == Some(&b'"') {
                    probe += 1;
                    loop {
                        if probe >= bytes.len() {
                            break;
                        }
                        if bytes[probe] == b'"'
                            && (0..hashes)
                                .all(|offset| bytes.get(probe + 1 + offset) == Some(&b'#'))
                        {
                            probe += 1 + hashes;
                            break;
                        }
                        probe += 1;
                    }
                    index = probe;
                } else {
                    index += 1;
                }
            }
            b'{' => {
                depth += 1;
                index += 1;
            }
            b'}' => {
                depth -= 1;
                index += 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => index += 1,
        }
    }
    None
}

/// The end of the item that starts at `item`, or `None` when it cannot be
/// delimited (in which case the caller keeps the text under the scan).
fn item_end(source: &str, item: usize) -> Option<usize> {
    let rest = &source[item..];
    let bytes = rest.as_bytes();
    if rest.starts_with("mod") {
        // `mod name;` declares an external test module and ends at the semicolon.
        // Treating it as a braced item would swallow whatever follows it.
        let mut cursor = 3usize;
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        while bytes
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            cursor += 1;
        }
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        return match bytes.get(cursor) {
            Some(b';') => Some(item + cursor + 1),
            Some(b'{') => block_end(source, item + cursor),
            _ => None,
        };
    }
    // Any other item: a braced body, or a `;`-terminated declaration.
    match (rest.find('{'), rest.find(';')) {
        (Some(open), Some(semi)) if semi < open => Some(item + semi + 1),
        (Some(open), _) => block_end(source, item + open),
        (None, Some(semi)) => Some(item + semi + 1),
        (None, None) => None,
    }
}

/// The source with every `#[cfg(test)]` item removed.
///
/// Items that cannot be delimited are left in place, so an unusual file is
/// scanned more strictly rather than less.
fn production_code(source: &str) -> String {
    let marker = "#[cfg(test)]";
    let mut remaining = String::with_capacity(source.len());
    let mut cursor = 0usize;
    while let Some(offset) = source[cursor..].find(marker) {
        let start = cursor + offset;
        remaining.push_str(&source[cursor..start]);
        let after = start + marker.len();
        // Skip further attributes and whitespace to reach the item's first token.
        let mut item = after;
        loop {
            let rest = &source[item..];
            let trimmed = rest.trim_start();
            item += rest.len() - trimmed.len();
            if trimmed.starts_with("#[") {
                match trimmed.find(']') {
                    Some(close) => item += close + 1,
                    None => break,
                }
            } else {
                break;
            }
        }
        match item_end(source, item) {
            Some(end) => {
                // Keep the line structure so a reported line number points at the
                // real file rather than at the stripped text.
                remaining.extend(
                    source[start..end]
                        .chars()
                        .filter(|character| *character == '\n'),
                );
                cursor = end;
            }
            None => {
                // Cannot delimit this item: keep it (and the rest) under the scan.
                remaining.extend(
                    source[start..after]
                        .chars()
                        .filter(|character| *character == '\n'),
                );
                cursor = after;
            }
        }
    }
    remaining.push_str(&source[cursor..]);
    remaining
}

fn banned_macros_in(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (index, line) in production_code(source).lines().enumerate() {
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

/// The weakness this test exists for: truncating the file at the first
/// `#[cfg(test)]` silently stopped scanning everything after it.
#[test]
fn production_code_after_a_test_module_is_still_scanned() {
    let source = "\
fn production() {}\n\
\n\
#[cfg(test)]\n\
mod tests {\n\
    fn t() { todo!() }\n\
}\n\
\n\
fn late_production() {\n\
    unreachable!(\"after the test module\");\n\
}\n";
    let found = banned_macros_in(source);
    assert_eq!(
        found.len(),
        1,
        "the late violation must be found: {found:?}"
    );
    assert!(found[0].contains("after the test module"));
}

#[test]
fn test_items_of_every_shape_are_removed_without_touching_production() {
    // Nested braces, braces in strings and in a raw string, and a char literal.
    let braced = "\
#[cfg(test)]\n\
mod tests {\n\
    fn t() {\n\
        let map = std::collections::HashMap::from([(1, \"}\" )]);\n\
        let raw = r#\" a } b \"#;\n\
        let ch = '}';\n\
        assert_eq!(map.len(), 1);\n\
        assert_eq!(raw.len(), 7);\n\
        assert_eq!(ch, '}');\n\
    }\n\
}\n\
fn after() {}\n";
    assert!(
        banned_macros_in(braced).is_empty(),
        "test-only code must be removed cleanly"
    );

    // An attribute on a single item, with production code after it.
    let single = "#[cfg(test)]\nfn helper() { todo!() }\n\nfn after() { unreachable!() }\n";
    let found = banned_macros_in(single);
    assert_eq!(found.len(), 1, "only the production line counts: {found:?}");
    assert!(found[0].contains("after"));

    // A file-based test module declares no body to remove.
    let file_module = "#[cfg(test)]\nmod tests;\n\nfn after() { unreachable!() }\n";
    assert_eq!(banned_macros_in(file_module).len(), 1);
}

/// An item whose end cannot be determined must be scanned, not skipped: a missed
/// violation is the failure mode that matters.
#[test]
fn an_undelimited_test_item_is_still_scanned() {
    let truncated = "#[cfg(test)]\nmod tests {\n    fn t() { unreachable!() }\n";
    assert_eq!(
        banned_macros_in(truncated).len(),
        1,
        "an unterminated test module must not hide its contents"
    );
}

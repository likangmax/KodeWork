//! Invariants for the danger-classification policy.
//!
//! `classify_danger` is a server-enforced review gate: a false negative lets a
//! destructive command run without confirmation. These tests pin the properties
//! the implementation claims (whitespace/case normalisation, composition always
//! reviewed, observational commands safe) without restating its keyword list, so
//! a regression in the normalisation or in the ordering of the checks fails here.

use kodework_domain::{classify_danger, DangerLevel};

/// Key phrases that must never be classified below `Dangerous`, including when
/// the user types them with different whitespace or letter case.
const DANGEROUS_PHRASES: [&str; 14] = [
    "rm -rf /",
    "rm -fr build",
    "git reset --hard HEAD~3",
    "git clean -fd",
    "git clean -xdf",
    "mkfs.ext4 /dev/sda1",
    "shutdown -h now",
    "reboot",
    "poweroff",
    "terraform destroy",
    "kubectl delete pod web",
    "helm uninstall release",
    "docker system prune -a",
    "git push --force origin main",
];

/// Whitespace variants that a shell treats as token separators. The policy
/// normalises them, so none of them may lower the classification.
fn whitespace_variants(phrase: &str) -> Vec<String> {
    let spaces = phrase.matches(' ').count();
    let mut variants = vec![phrase.to_string()];
    if spaces > 0 {
        variants.push(phrase.replace(' ', "\t"));
        variants.push(phrase.replace(' ', "  "));
        variants.push(phrase.replace(' ', " \t "));
        variants.push(format!("  {phrase}   "));
        variants.push(phrase.replace(' ', "\n"));
    }
    variants.push(phrase.to_ascii_uppercase());
    variants.push(
        phrase
            .chars()
            .enumerate()
            .map(|(index, character)| {
                if index % 2 == 0 {
                    character.to_ascii_uppercase()
                } else {
                    character
                }
            })
            .collect(),
    );
    variants
}

#[test]
fn dangerous_phrases_are_never_downgraded_by_formatting() {
    let mut failures = Vec::new();
    for phrase in DANGEROUS_PHRASES {
        let baseline = classify_danger(phrase);
        if baseline != DangerLevel::Dangerous {
            failures.push(format!(
                "{phrase:?} baselines at {baseline:?}, not Dangerous"
            ));
        }
        for variant in whitespace_variants(phrase) {
            let level = classify_danger(&variant);
            if level != DangerLevel::Dangerous {
                failures.push(format!("{variant:?} was downgraded to {level:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Shell composition can hide a side effect behind a harmless-looking command,
/// so it must always require review, never be treated as safe.
#[test]
fn shell_composition_always_requires_review() {
    let wrappers = [
        "echo hi && {}",
        "echo hi; {}",
        "echo hi | {}",
        "echo hi > out.txt; {}",
        "echo $(whoami); {}",
        "echo `id`; {}",
        "true & {}",
        "( {} )",
        "echo hi || {}",
    ];
    let bodies = ["ls", "cat README.md", "pwd", "echo done"];
    let mut failures = Vec::new();
    for wrapper in wrappers {
        for body in bodies {
            let command = wrapper.replace("{}", body);
            if classify_danger(&command) == DangerLevel::Safe {
                failures.push(format!("{command:?} was classified Safe"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The observational set stays Safe; the policy is a UX guardrail, so this also
/// documents that adding composition to one of them is what escalates it.
#[test]
fn observational_commands_stay_safe() {
    for command in [
        "pwd",
        "whoami",
        "git status",
        "git diff",
        "git log --oneline -5",
        "ls -la",
        "cat README.md",
        "echo hello",
        "tmux ls",
        "uname -a",
    ] {
        assert_eq!(
            classify_danger(command),
            DangerLevel::Safe,
            "{command:?} should stay observational"
        );
    }
}

#[test]
fn privilege_and_permission_changes_require_review() {
    for command in [
        "sudo apt install curl",
        "chmod 777 /tmp/x",
        "chown root:root /tmp/x",
        "systemctl restart nginx",
        "kill -9 1234",
    ] {
        assert_eq!(
            classify_danger(command),
            DangerLevel::Review,
            "{command:?} should require review"
        );
    }
}

/// Surrounding whitespace must not change the answer at all.
#[test]
fn stray_whitespace_does_not_change_the_result() {
    for command in
        DANGEROUS_PHRASES
            .iter()
            .copied()
            .chain(["pwd", "git status", "sudo ls", "echo a | cat"])
    {
        let expected = classify_danger(command);
        for padded in [
            format!(" {command}"),
            format!("{command} "),
            format!("\t{command}\t"),
        ] {
            assert_eq!(
                classify_danger(&padded),
                expected,
                "padding changed the classification of {command:?}"
            );
        }
    }
}

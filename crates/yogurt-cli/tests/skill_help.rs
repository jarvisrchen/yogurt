//! Keeps the yogurt-control skill's command block in line with the real `--help` output.
//! yogurt-cli has no lib target, so the built binary is the only way to introspect the clap tree.

use assert_cmd::Command;
use std::fs;

const START: &str = "<!-- yogurt-cli:start -->";
const END: &str = "<!-- yogurt-cli:end -->";
const SKILL_PATH: &str = "../../.claude/skills/yogurt-control/SKILL.md";

fn run_help(args: &[&str]) -> String {
    let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
    cmd.args(args).arg("--help");
    let output = cmd.assert().success();
    String::from_utf8(output.get_output().stdout.clone()).expect("help output is utf8")
}

/// Parses the "Commands:" section of a clap `--help` into subcommand names, dropping `help`.
fn subcommands(help: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_commands = false;
    for line in help.lines() {
        if !line.starts_with(' ') && !line.is_empty() {
            in_commands = line.trim_end() == "Commands:";
            continue;
        }
        if in_commands {
            if line.trim().is_empty() {
                break;
            }
            if let Some(name) = line.split_whitespace().next() {
                if name != "help" {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

fn about(help: &str) -> String {
    help.lines().next().unwrap_or("").trim().to_string()
}

fn generate_block() -> String {
    let mut out = String::new();
    for name in subcommands(&run_help(&[])) {
        let sub_help = run_help(&[&name]);
        out.push_str(&format!("- `yogurt {name}` - {}\n", about(&sub_help)));
        for nested in subcommands(&sub_help) {
            let nested_help = run_help(&[&name, &nested]);
            out.push_str(&format!(
                "  - `yogurt {name} {nested}` - {}\n",
                about(&nested_help)
            ));
        }
    }
    out.trim_end().to_string()
}

fn between_markers(text: &str) -> &str {
    let start = text.find(START).expect("skill is missing the start marker") + START.len();
    let end = text.find(END).expect("skill is missing the end marker");
    text[start..end].trim_matches('\n')
}

#[test]
fn generated_block_matches_skill_md() {
    let generated = generate_block();
    let skill = fs::read_to_string(SKILL_PATH).expect("read SKILL.md");
    let current = between_markers(&skill);

    if std::env::var("YOGURT_UPDATE_DOCS").as_deref() == Ok("1") {
        let updated = format!(
            "{}{}\n{}\n{}",
            &skill[..skill.find(START).unwrap() + START.len()],
            "",
            generated,
            &skill[skill.find(END).unwrap()..]
        );
        fs::write(SKILL_PATH, updated).expect("write SKILL.md");
        return;
    }

    assert_eq!(
        current, generated,
        "\nthe yogurt-cli block in {SKILL_PATH} is stale.\n\
         run `YOGURT_UPDATE_DOCS=1 cargo test -p yogurt --test skill_help` to regenerate it.\n"
    );
}

#[test]
fn every_yogurt_word_in_docs_is_a_real_subcommand() {
    let real = subcommands(&run_help(&[]));

    for path in [
        SKILL_PATH,
        "../../docs/AI-INTEGRATION.md",
        "../../README.md",
    ] {
        let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
        let mut in_fence = false;
        for (i, line) in text.lines().enumerate() {
            if line.starts_with("```") {
                in_fence = !in_fence;
                continue;
            }
            let words: Vec<String> = if in_fence {
                // A bare mention of "yogurt" mid-line (e.g. in an echo string) is not a command.
                first_word_after(line.trim_start(), "yogurt")
                    .into_iter()
                    .collect()
            } else {
                backticked_words_after(line, "yogurt")
            };
            for word in words {
                if word.starts_with('-') || real.contains(&word) {
                    continue;
                }
                panic!(
                    "{path}:{}: `yogurt {word}` is not a real subcommand (have: {real:?})",
                    i + 1
                );
            }
        }
    }
}

fn first_word_after(line: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(prefix)?.strip_prefix(' ')?;
    rest.split_whitespace().next().map(str::to_string)
}

fn backticked_words_after(line: &str, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = format!("`{prefix} ");
    let mut rest = line;
    while let Some(pos) = rest.find(&needle) {
        let after = &rest[pos + needle.len()..];
        if let Some(word) = after
            .split(|c: char| !c.is_alphanumeric() && c != '-')
            .next()
        {
            if !word.is_empty() {
                out.push(word.to_string());
            }
        }
        rest = &rest[pos + needle.len()..];
    }
    out
}

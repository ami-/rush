use std::cell::RefCell;
use std::collections::HashMap;
use std::process::Command;
use std::rc::Rc;

use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::config::BellStyle;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Config, Context, Editor, Helper};

use crate::{BUILTINS, executables_with_prefix};

pub struct ShellHelper {
    file_completer: FilenameCompleter,
    completions: Rc<RefCell<HashMap<String, String>>>,
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let before = &line[..pos];
        let word_start = before
            .rfind(|c: char| c.is_ascii_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        if word_start != 0 {
            //check builtin registered completions
            let cmd_name = line.split_ascii_whitespace().next().unwrap_or("");
            let current_word = &line[word_start..pos];
            let before_current = &line[..word_start.saturating_sub(1)];
            let prev_word_start = before_current
                .rfind(|c: char| c.is_ascii_whitespace())
                .map(|i| i + 1)
                .unwrap_or(0);
            let prev_word = &before_current[prev_word_start..];
            if let Some(cmd_path) = self.completions.borrow().get(cmd_name) {
                let candidates: Vec<Pair> = external_candidates(
                    cmd_name,
                    cmd_path,
                    current_word,
                    prev_word,
                    line,
                    &format!("{}", pos),
                );
                if !candidates.is_empty() {
                    return Ok((word_start, candidates));
                }
            };

            //file completions
            let (start, mut candidates) = self.file_completer.complete(line, pos, ctx)?;
            for c in &mut candidates {
                if c.replacement.ends_with('/') {
                    // on dirs / is in replacement but not in display
                    c.display.push('/');
                } else if !c.replacement.ends_with(' ') {
                    //make sure to add ' ' at the end of replacement
                    c.replacement.push(' ');
                }
            }
            return Ok((start, candidates));
        }
        let prefix = &line[..pos];
        let mut seen = std::collections::HashSet::new();
        let mut candidates: Vec<Pair> = BUILTINS
            .iter()
            .filter(|&&b| b.starts_with(prefix))
            .map(|&b| {
                seen.insert(b.to_string());
                Pair {
                    display: b.to_string(),
                    replacement: format!("{} ", b),
                }
            })
            .collect();
        for name in executables_with_prefix(prefix) {
            if seen.insert(name.clone()) {
                candidates.push(Pair {
                    display: name.clone(),
                    replacement: format!("{} ", name),
                });
            }
        }
        Ok((0, candidates))
    }
}

impl Hinter for ShellHelper {
    type Hint = String;
}
impl Highlighter for ShellHelper {}
impl Validator for ShellHelper {}
impl Helper for ShellHelper {}

pub fn create_editor(
    completions: Rc<RefCell<HashMap<String, String>>>,
    ignore_duplicates: bool,
) -> rustyline::Result<Editor<ShellHelper, DefaultHistory>> {
    let config = Config::builder()
        .auto_add_history(true)
        .history_ignore_dups(ignore_duplicates)?
        .history_ignore_space(true)
        .bell_style(BellStyle::Audible)
        .completion_type(rustyline::CompletionType::List)
        .build();
    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(ShellHelper {
        file_completer: FilenameCompleter::new(),
        completions: completions,
    }));
    Ok(rl)
}

fn external_candidates(
    cmd: &str,
    path: &str,
    current: &str,
    prev: &str,
    comp_line: &str,
    comp_point: &str,
) -> Vec<Pair> {
    let args: Vec<&str> = vec![cmd, current, prev];
    if let Ok(output) = Command::new(path)
        .env("COMP_LINE", comp_line)
        .env("COMP_POINT", comp_point)
        .args(args)
        .output()
    {
        let out = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| Pair {
                display: line.to_string(),
                replacement: format!("{} ", line),
            })
            .collect();
        return out;
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyline::history::DefaultHistory;

    fn complete(line: &str) -> Vec<Pair> {
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (_, candidates) = ShellHelper {
            file_completer: FilenameCompleter::new(),
            completions: Rc::new(RefCell::new(HashMap::new())),
        }
        .complete(line, line.len(), &ctx)
        .unwrap();
        candidates
    }

    fn replacements(line: &str) -> Vec<String> {
        complete(line).into_iter().map(|p| p.replacement).collect()
    }

    #[test]
    fn empty_prefix_includes_all_builtins() {
        let r = replacements("");
        for builtin in crate::BUILTINS {
            assert!(
                r.contains(&format!("{} ", builtin)),
                "missing builtin: {}",
                builtin
            );
        }
    }

    #[test]
    fn builtin_prefix_includes_builtin() {
        assert!(replacements("ec").contains(&"echo ".to_string()));
        assert!(replacements("pw").contains(&"pwd ".to_string()));
        assert!(replacements("cd").contains(&"cd ".to_string()));
    }

    #[test]
    fn ambiguous_builtin_prefix_includes_all_matches() {
        let r = replacements("e");
        assert!(r.contains(&"echo ".to_string()));
        assert!(r.contains(&"exit ".to_string()));
    }

    #[test]
    fn exact_match_still_appends_space() {
        assert!(replacements("echo").contains(&"echo ".to_string()));
    }

    #[test]
    fn no_match_returns_empty() {
        assert!(replacements("thisprefixdoesnotexist__").is_empty());
    }

    #[test]
    fn builtins_not_duplicated_when_also_in_path() {
        let r = replacements("echo");
        assert_eq!(r.iter().filter(|s| s.as_str() == "echo ").count(), 1);
    }

    #[test]
    fn start_position_is_zero_on_match() {
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (start, _) = ShellHelper {
            file_completer: FilenameCompleter::new(),
            completions: Rc::new(RefCell::new(HashMap::new())),
        }
        .complete("ec", 2, &ctx)
        .unwrap();
        assert_eq!(start, 0);
    }
}

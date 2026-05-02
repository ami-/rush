use rustyline::completion::{Completer, Pair};
use rustyline::config::BellStyle;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Config, Context, Editor, Helper};

use crate::{BUILTINS, executables_with_prefix};

pub struct ShellHelper;

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let before = &line[..pos];
        let word_start = before
            .rfind(|c: char| c.is_ascii_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        if word_start != 0 {
            return Ok((pos, vec![]));
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

pub fn create_editor() -> rustyline::Result<Editor<ShellHelper, DefaultHistory>> {
    let config = Config::builder()
        .auto_add_history(true)
        .history_ignore_dups(true)?
        .history_ignore_space(true)
        .bell_style(BellStyle::Audible)
        .completion_type(rustyline::CompletionType::List)
        .build();
    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(ShellHelper));
    Ok(rl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyline::history::DefaultHistory;

    fn complete(line: &str) -> Vec<Pair> {
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (_, candidates) = ShellHelper.complete(line, line.len(), &ctx).unwrap();
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
    fn mid_argument_returns_empty() {
        assert!(complete("echo ").is_empty());
        assert!(complete("echo he").is_empty());
        assert!(complete("type ec").is_empty());
    }

    #[test]
    fn start_position_is_zero_on_match() {
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (start, _) = ShellHelper.complete("ec", 2, &ctx).unwrap();
        assert_eq!(start, 0);
    }
}

use rustyline::completion::{Completer, Pair};
use rustyline::config::BellStyle;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Config, Context, Editor, Helper};

use crate::BUILTINS;

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
        let candidates = BUILTINS
            .iter()
            .filter(|&&b| b.starts_with(prefix))
            .map(|&b| Pair {
                display: b.to_string(),
                replacement: format!("{} ", b),
            })
            .collect();
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
    fn empty_prefix_returns_all_builtins() {
        let mut r = replacements("");
        r.sort();
        assert_eq!(r, vec!["cd ", "echo ", "exit ", "pwd ", "type "]);
    }

    #[test]
    fn unique_prefix_completes_with_space() {
        assert_eq!(replacements("ec"), vec!["echo "]);
        assert_eq!(replacements("pw"), vec!["pwd "]);
        assert_eq!(replacements("cd"), vec!["cd "]);
    }

    #[test]
    fn ambiguous_prefix_returns_all_matches() {
        let mut r = replacements("e");
        r.sort();
        assert_eq!(r, vec!["echo ", "exit "]);
    }

    #[test]
    fn exact_match_still_appends_space() {
        assert_eq!(replacements("echo"), vec!["echo "]);
    }

    #[test]
    fn no_match_returns_empty() {
        assert!(replacements("xyz").is_empty());
        assert!(replacements("z").is_empty());
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

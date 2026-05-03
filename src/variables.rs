use regex::{Captures, Regex};
use std::collections::HashMap;

pub fn expand_vars(decls: &HashMap<String, String>, tokens: &mut Vec<String>) {
    // Group 1: $NAME style | Group 2: ${NAME} style
    let re = Regex::new(r"\$(\w+)|\$\{(\w+)\}").unwrap();
    *tokens = tokens
        .iter()
        .map(|arg| {
            re.replace_all(arg, |cap: &Captures| {
                let var = cap
                    .get(1)
                    .or_else(|| cap.get(2))
                    .map(|m| m.as_str())
                    .unwrap_or("");

                decls.get(var).map(|s| s.as_str()).unwrap_or("").to_string()
            })
            .into_owned()
        })
        .filter(|s| !s.is_empty())
        .collect();
}
#[cfg(test)]
mod tests {
    use super::*;

    fn decls(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn expand(decls: &HashMap<String, String>, args: &[&str]) -> Vec<String> {
        let mut tokens: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        expand_vars(decls, &mut tokens);
        tokens
    }

    #[test]
    fn simple_var() {
        let d = decls(&[("FOO", "bar")]);
        assert_eq!(expand(&d, &["$FOO"]), vec!["bar"]);
    }

    #[test]
    fn braced_var() {
        let d = decls(&[("FOO", "bar")]);
        assert_eq!(expand(&d, &["${FOO}"]), vec!["bar"]);
    }

    #[test]
    fn var_inline() {
        let d = decls(&[("FOO", "bar")]);
        assert_eq!(expand(&d, &["prefix_$FOO"]), vec!["prefix_bar"]);
    }

    #[test]
    fn undefined_var_removed() {
        let d = decls(&[]);
        assert_eq!(expand(&d, &["$UNDEF"]), Vec::<String>::new());
    }

    #[test]
    fn undefined_var_inline_not_removed() {
        let d = decls(&[]);
        assert_eq!(expand(&d, &["prefix_$UNDEF"]), vec!["prefix_"]);
    }

    #[test]
    fn multiple_vars_in_one_token() {
        let d = decls(&[("A", "hello"), ("B", "world")]);
        assert_eq!(expand(&d, &["$A $B"]), vec!["hello world"]);
    }

    #[test]
    fn unaffected_tokens_preserved() {
        let d = decls(&[("FOO", "bar")]);
        assert_eq!(expand(&d, &["plain", "$FOO"]), vec!["plain", "bar"]);
    }
}

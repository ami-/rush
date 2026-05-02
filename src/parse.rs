pub fn parse_cmd(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut in_sq = false;
    let mut in_dq = false;

    const SQ: char = '\'';
    const DQ: char = '"';
    const BS: char = '\\';

    let mut buf = String::new();

    let mut cn: char;
    let mut it = line.chars().peekable();
    while let Some(cc) = it.next() {
        cn = ' ';
        let mut end = true;
        if let Some(&c) = it.peek() {
            cn = c;
            end = false;
        }
        if cc == BS && !in_sq && !in_dq {
            if end {
                continue;
            }
            buf.push(cn);
            let _ = it.next();
            continue;
        }
        if cc == BS && in_dq {
            if end {
                continue;
            }
            if cn == DQ || cn == BS || cn == '$' || cn == '`' || cn == '\n' {
                buf.push(cn);
                let _ = it.next();
                continue;
            }
        }
        if !in_dq {
            if cc == SQ && cn == SQ {
                let _ = it.next();
                continue;
            }
            if cc == SQ && cn != SQ {
                in_sq = !in_sq;
                continue;
            }
        }
        if !in_sq {
            if cc == DQ && cn == DQ {
                let _ = it.next();
                continue;
            }
            if cc == DQ && cn != DQ {
                in_dq = !in_dq;
                continue;
            }
        }
        if !in_sq && !in_dq && cc.is_ascii_whitespace() {
            if buf.len() > 0 {
                out.push(buf.clone());
                buf.clear();
            }
            continue;
        }
        if !in_sq && !in_dq && cc == '|' {
            if buf.len() > 0 {
                out.push(buf.clone());
                buf.clear();
            }
            out.push("|".to_string());
            continue;
        }
        if !in_sq && !in_dq && cc == '&' {
            if buf.ends_with('>') {
                // part of 2>&1 or >&2 — keep glued to the buffer
                buf.push('&');
                continue;
            }
            if cn == '>' {
                if !buf.is_empty() {
                    out.push(buf.clone());
                    buf.clear();
                }
                let _ = it.next(); // consume '>'
                if it.peek() == Some(&'>') {
                    let _ = it.next();
                    out.push("&>>".to_string());
                } else {
                    out.push("&>".to_string());
                }
                continue;
            }
            if !buf.is_empty() {
                out.push(buf.clone());
                buf.clear();
            }
            out.push("&".to_string());
            continue;
        }
        buf.push(cc);
    }
    if buf.len() > 0 {
        out.push(buf)
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_squotes() {
        let result = parse_cmd("hello");
        assert_eq!(result[0], "hello");
    }
    #[test]
    fn empty_squotes() {
        let result = parse_cmd("''hello");
        assert_eq!(result[0], "hello");
    }
    #[test]
    fn simple_squotes() {
        let result = parse_cmd("'hello'    'bau'");
        assert_eq!(result[0], "hello");
        assert_eq!(result[1], "bau");
    }
    #[test]
    fn concat_squotes() {
        let result = parse_cmd("first 'hello''bau' 'cucu'");
        assert_eq!(result[0], "first");
        assert_eq!(result[1], "hellobau");
        assert_eq!(result[2], "cucu");
    }
    #[test]
    fn multi_squotes_inside() {
        let result = parse_cmd("'hello''''bau'");
        assert_eq!(result[0], "hellobau");
    }
    #[test]
    fn multi_squotes_outside() {
        let result = parse_cmd("hello''''bau");
        assert_eq!(result[0], "hellobau");
    }
    #[test]
    fn multi_squotes2() {
        let result = parse_cmd("echo 'shell     test' 'hello''example' script''world");
        assert_eq!(result[0], "echo");
        assert_eq!(result[1], "shell     test");
        assert_eq!(result[2], "helloexample");
        assert_eq!(result[3], "scriptworld");
    }
    #[test]
    fn preserve_space() {
        let result = parse_cmd("'hello    world'");
        assert_eq!(result[0], "hello    world");
    }
    #[test]
    fn simple_dquote() {
        let result = parse_cmd(r#""hello    world""#);
        assert_eq!(result[0], "hello    world");
    }
    #[test]
    fn multi1_dquoe() {
        let result = parse_cmd(r#""hello" 'bau'   "world""#);
        assert_eq!(result[0], "hello");
        assert_eq!(result[1], "bau");
        assert_eq!(result[2], "world");
    }
    #[test]
    fn multi2_dquoe() {
        let result = parse_cmd(r#""hello""world""#);
        assert_eq!(result[0], "helloworld");
    }
    #[test]
    fn combined_dquoe() {
        let result = parse_cmd(r#""hell's kitchen""#);
        assert_eq!(result[0], "hell's kitchen");
    }
    #[test]
    fn combined2_dquoe() {
        let result = parse_cmd(r#""'inside'""#);
        assert_eq!(result[0], r#"'inside'"#);
    }
    #[test]
    fn escape() {
        let result = parse_cmd(r#"\'\"literal quotes\"\'"#);
        assert_eq!(result[0], r#"'"literal"#);
        assert_eq!(result[1], r#"quotes"'"#);
    }
    #[test]
    fn backslash_in_squote() {
        let result = parse_cmd("'shell\\\nscript'");
        assert_eq!(result[0], "shell\\\nscript");
    }
    #[test]
    fn backslash_in_dquote() {
        let result = parse_cmd(r#""just'one'\\n'backslash""#);
        assert_eq!(result[0], r#"just'one'\n'backslash"#);
    }
}
pub fn split_pipeline(tokens: Vec<String>) -> Vec<Vec<String>> {
    let mut segs: Vec<Vec<String>> = Vec::new();
    let mut seg: Vec<String> = Vec::new();
    for token in tokens {
        if token == "|" {
            segs.push(seg);
            seg = Vec::new();
        } else {
            seg.push(token);
        }
    }
    segs.push(seg);
    segs
}

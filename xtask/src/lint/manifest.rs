//! Just enough of TOML to read the parts of `Cargo.toml` and `Cargo.lock`
//! the lints need: table headers, `key = value` entries (a value may be an
//! array spanning lines) and the strings inside a value. It is a line
//! reader, not a TOML parser; it understands what cargo writes and what
//! the workspace's manifests use, and nothing more.

/// One `key = value` entry.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The table header it is under, as written between the brackets,
    /// without surrounding spaces (`""` before any header).
    pub table: String,
    /// The key, quotes removed.
    pub key: String,
    /// The raw value text (for a multi-line array, the lines joined).
    pub value: String,
    /// The 1-based line of the key.
    pub line: usize,
}

/// A table header.
#[derive(Debug, Clone)]
pub struct Header {
    pub table: String,
    /// `[[array]]` rather than `[table]`.
    pub array: bool,
    pub line: usize,
}

/// The headers and entries of `text`, in order.
pub fn parse(text: &str) -> (Vec<Header>, Vec<Entry>) {
    let mut headers = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    let mut table = String::new();
    let mut open: Option<usize> = None; // entry still collecting an array
    for (i, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if let Some(at) = open {
            let e = &mut entries[at];
            e.value.push(' ');
            e.value.push_str(line);
            if balanced(&e.value) {
                open = None;
            }
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if let Some(inner) = line.strip_prefix("[[").and_then(|l| l.strip_suffix("]]")) {
            inner.trim().clone_into(&mut table);
            headers.push(Header { table: table.clone(), array: true, line: i + 1 });
        } else if let Some(inner) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            inner.trim().clone_into(&mut table);
            headers.push(Header { table: table.clone(), array: false, line: i + 1 });
        } else if let Some((key, value)) = split_key(line) {
            entries.push(Entry {
                table: table.clone(),
                key: unquote(key.trim()),
                value: value.trim().to_owned(),
                line: i + 1,
            });
            if !balanced(value) {
                open = Some(entries.len() - 1);
            }
        }
    }
    (headers, entries)
}

/// The strings (`"..."` or `'...'`) inside a value, in order.
pub fn strings(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '"' || c == '\'' {
            let mut s = String::new();
            while let Some(d) = chars.next() {
                if d == c {
                    break;
                }
                if d == '\\' && c == '"' {
                    if let Some(e) = chars.next() {
                        s.push(e);
                    }
                    continue;
                }
                s.push(d);
            }
            out.push(s);
        }
    }
    out
}

/// The value of key `key` inside an inline table value (`{ a = "x" }`), as
/// a string, if it is one.
pub fn inline_string(value: &str, key: &str) -> Option<String> {
    let inner = value.trim().strip_prefix('{')?.strip_suffix('}')?;
    for part in inner.split(',') {
        if let Some((k, v)) = split_key(part)
            && unquote(k.trim()) == key
        {
            return strings(v).into_iter().next();
        }
    }
    None
}

fn split_key(line: &str) -> Option<(&str, &str)> {
    // The first `=` outside quotes.
    let mut quote: Option<char> = None;
    for (i, c) in line.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '=' => return Some((&line[..i], &line[i + 1..])),
            Some(_) | None => {}
        }
    }
    None
}

fn unquote(key: &str) -> String {
    key.split('.')
        .map(|part| part.trim().trim_matches(|c| c == '"' || c == '\''))
        .collect::<Vec<_>>()
        .join(".")
}

fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    for (i, c) in line.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' => return &line[..i],
            Some(_) | None => {}
        }
    }
    line
}

/// Whether the brackets and braces outside strings in `value` close.
fn balanced(value: &str) -> bool {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for c in value.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                _ => {}
            },
        }
    }
    depth <= 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tables_entries_and_arrays() {
        let (h, e) = parse(
            "[workspace]\nmembers = [\n  \"crates/*\", # c\n  \"xtask\",\n]\n\
             [dependencies]\na.workspace = true\nb = { path = \"x\", package = \"c\" }\n",
        );
        assert_eq!(h.len(), 2);
        assert_eq!(strings(&e[0].value), ["crates/*", "xtask"]);
        assert_eq!((e[1].key.as_str(), e[1].line), ("a.workspace", 7));
        assert_eq!(inline_string(&e[2].value, "package").as_deref(), Some("c"));
    }
}

//! Reading the Make rule Kiln writes for `-MD -MF` (`kiln/spec/driver.md`
//! "Dependency output"), to list the files a parse read
//! (`spec/landed/model/WP-010.md`).
//!
//! Kiln writes one rule, `<target>: <file> <file> …`, lines continued with
//! ` \`, file names escaped as clang's `DependencyFileGenerator` does
//! (`kiln_driver::deps::escape_filename`): `#` as `\#`, `$` as `$$`, a space
//! as `\ ` with every backslash before it doubled. The target is the
//! `-MT` the parse task passes, so where the rule's list starts is known.

/// The prerequisites of the rule in `text` whose target is `target`, in
/// order, unescaped. `Err` if `text` is not one such rule.
pub(crate) fn prerequisites(text: &str, target: &str) -> Result<Vec<String>, String> {
    let head = format!("{target}:");
    let Some(rest) = text.strip_prefix(&head) else {
        return Err(format!("the dependency file does not start with `{head}`"));
    };
    // Line continuations: a backslash immediately before a newline.
    let joined = rest.replace("\\\r\n", " ").replace("\\\n", " ");
    let mut lines = joined.split('\n');
    let list = lines.next().unwrap_or("");
    if lines.any(|l| !l.trim().is_empty()) {
        return Err("the dependency file holds more than one rule".into());
    }

    let b: Vec<char> = list.chars().collect();
    let mut files = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            '\\' => {
                let start = i;
                while i < b.len() && b[i] == '\\' {
                    i += 1;
                }
                let n = i - start;
                match b.get(i) {
                    // 2k+1 backslashes and a space: k backslashes and a space;
                    // 2k: k backslashes, then the space separates.
                    Some(' ') => {
                        word.extend(std::iter::repeat_n('\\', n / 2));
                        in_word = true;
                        if n % 2 == 1 {
                            word.push(' ');
                            i += 1;
                        }
                    }
                    Some('#') if n == 1 => {
                        word.push('#');
                        in_word = true;
                        i += 1;
                    }
                    _ => {
                        word.extend(std::iter::repeat_n('\\', n));
                        in_word = true;
                    }
                }
                continue;
            }
            '$' if b.get(i + 1) == Some(&'$') => {
                word.push('$');
                in_word = true;
                i += 2;
                continue;
            }
            ' ' | '\t' | '\r' => {
                if in_word {
                    files.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            c => {
                word.push(c);
                in_word = true;
            }
        }
        i += 1;
    }
    if in_word {
        files.push(word);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::prerequisites;
    use kiln_driver::deps::render;

    // The inverse of Kiln's own rendering, wrapping included.
    #[test]
    fn inverts_kiln_rendering() {
        let names: Vec<String> = [
            "main.cpp",
            "a b.h",
            "c#d.h",
            "e$f.h",
            "g\\ h.h",
            "dir with  two/x.h",
            &format!("/long/{}.h", "y".repeat(90)),
            "back\\slash.h",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        for phony in [false, true] {
            let text = render(&["slip.d".to_owned()], &names, phony);
            if phony {
                // -MP adds rules; the parse task never passes it.
                assert!(prerequisites(&text, "slip.d").is_err());
            } else {
                assert_eq!(prerequisites(&text, "slip.d").unwrap(), names);
            }
        }
    }

    #[test]
    fn wrong_target_is_an_error() {
        assert!(prerequisites("x.o: a.c\n", "slip.d").is_err());
        assert_eq!(prerequisites("slip.d:\n", "slip.d").unwrap(), Vec::<String>::new());
    }
}

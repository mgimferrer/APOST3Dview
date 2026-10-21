//! The user guide's text (`docs/src/`) against the app: every label the
//! guide writes in bold exists in the app's code, the guide has no en or em
//! dashes, and every picture and link in it resolves. (The steps themselves
//! are followed in the real app by `tests/guide.rs`.) Needs no GPU.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn files_with_extension(dir: &Path, extension: &str, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_with_extension(&path, extension, found);
        } else if path.extension().is_some_and(|e| e == extension) {
            found.push(path);
        }
    }
}

/// The guide's pages: (path, text).
fn pages() -> Vec<(PathBuf, String)> {
    let mut paths = Vec::new();
    files_with_extension(&repo().join("docs/src"), "md", &mut paths);
    paths.sort();
    assert!(paths.len() > 10, "the guide's pages are missing");
    paths.into_iter().map(|p| {
        let text = std::fs::read_to_string(&p).unwrap();
        (p, text)
    }).collect()
}

/// Text without inline code (`...`), where ** and labels mean nothing.
fn without_code(line: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for c in line.chars() {
        if c == '`' {
            in_code = !in_code;
        } else if !in_code {
            out.push(c);
        }
    }
    out
}

/// Every `**bold**` span of the page.
fn bold_spans(text: &str) -> Vec<String> {
    let mut spans = Vec::new();
    for line in text.lines() {
        let line = without_code(line);
        let mut rest = line.as_str();
        while let Some(start) = rest.find("**") {
            let after = &rest[start + 2..];
            let Some(end) = after.find("**") else { break };
            spans.push(after[..end].to_string());
            rest = &after[end + 2..];
        }
    }
    spans
}

/// Every string literal in the app's source, escapes resolved.
fn app_strings() -> Vec<String> {
    let mut paths = Vec::new();
    files_with_extension(&repo().join("crates/app/src"), "rs", &mut paths);
    let mut strings = Vec::new();
    for path in paths {
        let source = std::fs::read_to_string(path).unwrap();
        let mut chars = source.chars().peekable();
        let mut previous = ' ';
        while let Some(c) = chars.next() {
            // Skip character literals such as '"', and line comments.
            if c == '\'' && previous != '\\' {
                let lookahead: String = chars.clone().take(3).collect();
                if lookahead.starts_with("\"'") {
                    chars.next();
                    chars.next();
                    previous = '\'';
                    continue;
                }
            }
            if c == '/' && chars.peek() == Some(&'/') {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
                previous = '\n';
                continue;
            }
            if c == '"' {
                let mut literal = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => literal.push('\n'),
                            Some('u') => {
                                let code: String = chars.by_ref().skip(1).take_while(|c| *c != '}').collect();
                                if let Some(c) = u32::from_str_radix(&code, 16).ok().and_then(char::from_u32) {
                                    literal.push(c);
                                }
                            }
                            Some('\n') => {
                                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                                    chars.next();
                                }
                            }
                            Some(other) => literal.push(other),
                            None => break,
                        },
                        other => literal.push(other),
                    }
                }
                strings.push(literal);
                previous = '"';
                continue;
            }
            previous = c;
        }
    }
    strings
}

/// Whether `label` matches the format string `literal`, its `{...}`
/// placeholders standing for any text.
fn matches_format(label: &str, literal: &str) -> bool {
    let mut parts = Vec::new();
    let mut rest = literal;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else { break };
        parts.push(&rest[..open]);
        rest = &rest[open + close + 1..];
    }
    parts.push(rest);
    // A format string of placeholders only ("{} {}") would match anything.
    if parts.len() < 2 || parts.iter().map(|p| p.trim().len()).sum::<usize>() < 4 {
        return false;
    }
    let mut position = 0;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !label.starts_with(part) {
                return false;
            }
            position = part.len();
        } else if i == parts.len() - 1 {
            return label.len() >= position + part.len() && label.ends_with(part) && label.len() > position;
        } else {
            match label[position..].find(part) {
                Some(found) if found > 0 => position += found + part.len(),
                _ => return false,
            }
        }
    }
    true
}

/// A label the guide names is in the app: as a literal, the literal
/// without its colon, a format string with values filled in, or the fixed
/// start of a format string ("Generate" for "Generate {n} orbital(s)").
fn in_app(label: &str, strings: &[String]) -> bool {
    strings.iter().any(|s| {
        let s = s.trim();
        s == label
            || s.strip_suffix(':') == Some(label)
            || matches_format(label, s)
            || s.find('{').is_some_and(|i| i >= 3 && s[..i].trim_end_matches(['(', ' ']) == label)
    })
}

#[test]
fn every_bold_label_is_in_the_app() {
    let strings = app_strings();
    let mut missing = BTreeSet::new();
    for (path, text) in pages() {
        for label in bold_spans(&text) {
            if !in_app(&label, &strings) {
                missing.insert(format!("{}: {label}", path.strip_prefix(repo().join("docs/src")).unwrap().display()));
            }
        }
    }
    assert!(missing.is_empty(), "labels in bold in the guide that the app doesn't show (rename them in the guide, or write them in italics if they come from a results file):\n{}", missing.into_iter().collect::<Vec<_>>().join("\n"));
}

#[test]
fn no_en_or_em_dashes() {
    for (path, text) in pages() {
        for (n, line) in text.lines().enumerate() {
            assert!(!line.contains(['\u{2013}', '\u{2014}']), "{}:{}: an en or em dash: {line}", path.display(), n + 1);
        }
    }
}

/// Targets of the page's links and pictures: `](target)` and `src="target"`.
fn targets(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (open, close) in [("](", ')'), ("src=\"", '"')] {
        let mut rest = text;
        while let Some(start) = rest.find(open) {
            let after = &rest[start + open.len()..];
            let Some(end) = after.find(close) else { break };
            found.push(after[..end].to_string());
            rest = &after[end..];
        }
    }
    found
}

#[test]
fn every_picture_and_link_resolves() {
    let images = repo().join("docs/src/images");
    let mut used = BTreeSet::new();
    for (path, text) in pages() {
        let dir = path.parent().unwrap();
        for target in targets(&text) {
            if target.starts_with("http") || target.starts_with("mailto:") || target.starts_with('#') {
                continue;
            }
            let file = target.split('#').next().unwrap();
            let resolved = dir.join(file);
            assert!(resolved.is_file(), "{}: {target} doesn't exist", path.display());
            if let Ok(canonical) = resolved.canonicalize() {
                used.insert(canonical);
            }
        }
    }
    let mut pictures = Vec::new();
    files_with_extension(&images, "png", &mut pictures);
    for picture in pictures {
        assert!(used.contains(&picture.canonicalize().unwrap()), "{} isn't used by any page (delete it, or regenerate the pictures)", picture.display());
    }
}

#[test]
fn every_page_is_in_the_summary() {
    let src = repo().join("docs/src");
    let summary = std::fs::read_to_string(src.join("SUMMARY.md")).unwrap();
    let listed: BTreeSet<String> = targets(&summary).into_iter().collect();
    for (path, _) in pages() {
        let relative = path.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
        if relative != "SUMMARY.md" {
            assert!(listed.contains(&relative), "{relative} isn't in SUMMARY.md");
        }
    }
}

#[test]
fn format_matching() {
    assert!(matches_format("Generate 2 orbital(s)", "Generate {selected_count} orbital(s)"));
    assert!(matches_format("Save FeCN5NO3.inp", "Save {file_name}"));
    assert!(!matches_format("Generate orbital(s)", "Generate {n} orbital(s)"));
    assert!(in_app("Generate", &["Generate {n} orbital(s)".to_string()]));
    assert!(in_app("Range", &["Range:".to_string()]));
    assert!(!in_app("Nonsense", &["Generate {n} orbital(s)".to_string()]));
    assert!(!in_app("Make coffee", &["{} {}".to_string(), "{name}".to_string()]));
}

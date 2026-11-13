//! Release notes arrive as HTML, Markdown or plain text. Camu shows them as
//! plain text: rendering a stranger's HTML inside the app would let any feed
//! run code with the app's privileges.

/// Turn release-note HTML into readable plain text.
pub fn to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            rest = "";
            break;
        };
        let tag = rest[start + 1..start + end].trim().to_ascii_lowercase();
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        rest = &rest[start + end + 1..];

        // Drop the contents of tags that are never text.
        if !tag.starts_with('/') && matches!(name.as_str(), "script" | "style" | "head" | "title") {
            let close = format!("</{name}");
            match rest.to_ascii_lowercase().find(&close) {
                Some(i) => rest = &rest[i..],
                None => rest = "",
            }
            continue;
        }
        match name.as_str() {
            "br" | "p" | "div" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" => {
                out.push('\n')
            }
            "li" if !tag.starts_with('/') => out.push_str("\n• "),
            _ => {}
        }
    }
    out.push_str(rest);
    tidy(&decode_entities(&out))
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        // An entity is short ("&amp;", "&#x4e2d;"): look for its ";" within
        // the next few characters. Counting bytes could cut a character in two.
        let window = rest.char_indices().nth(10).map_or(rest.len(), |(i, _)| i);
        let Some(semi) = rest[..window].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "hellip" => Some('…'),
            "bull" => Some('•'),
            _ => entity
                .strip_prefix("#x")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Collapse runs of spaces and blank lines, and trim each line.
fn tidy(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() && lines.last().is_none_or(|l| l.is_empty()) {
            continue;
        }
        lines.push(line);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::to_text;

    #[test]
    fn lists_and_paragraphs_become_lines() {
        let html = "<h2>What's new</h2><ul><li>Faster &amp; smaller</li><li>Fixed a crash</li></ul><p>Thanks!</p>";
        assert_eq!(
            to_text(html),
            "What's new\n\n• Faster & smaller\n• Fixed a crash\n\nThanks!"
        );
    }

    #[test]
    fn scripts_and_styles_are_dropped() {
        let html = "<style>p{color:red}</style><script>alert(1)</script><p>Safe</p>";
        assert_eq!(to_text(html), "Safe");
    }

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(
            to_text("  Bug fixes  and\n\n\n\nimprovements "),
            "Bug fixes and\n\nimprovements"
        );
        assert_eq!(to_text("Tom & Jerry &#8212; &#x2713;"), "Tom & Jerry — ✓");
    }
}

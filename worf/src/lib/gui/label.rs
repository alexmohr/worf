//! Parsing and formatting of entry labels.

/// Truncates pango markup to `max_len` visible chars and appends `...`.
/// Tags are not counted and stay intact, entities like `&amp;` count as one char.
/// Returns `None` if the text is short enough.
pub(super) fn truncate_markup(markup: &str, max_len: usize) -> Option<String> {
    let mut result = String::with_capacity(markup.len());
    let mut open_tags: Vec<&str> = Vec::new();
    let mut visible = 0;
    let mut rest = markup;

    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some(end) = rest.find('>')
        {
            let tag = &rest[..=end];
            let inner = tag[1..tag.len() - 1].trim();
            if let Some(closing) = inner.strip_prefix('/') {
                if open_tags.last() == Some(&closing.trim()) {
                    open_tags.pop();
                }
            } else if !inner.ends_with('/') {
                open_tags.push(inner.split_whitespace().next().unwrap_or_default());
            }
            result.push_str(tag);
            rest = &rest[end + 1..];
            continue;
        }

        if visible == max_len {
            result.push_str("...");
            for tag in open_tags.iter().rev() {
                result.push_str("</");
                result.push_str(tag);
                result.push('>');
            }
            return Some(result);
        }

        let len = if c == '&'
            && let Some(end) = rest.find(';')
            && end <= 10
            && !rest[..end].contains(char::is_whitespace)
        {
            end + 1
        } else {
            c.len_utf8()
        };
        result.push_str(&rest[..len]);
        rest = &rest[len..];
        visible += 1;
    }

    None
}

/// Parses labels in the wofi format `img:<path>:text:<text>`.
/// Labels that do not start with `img:` or `text:` are plain text.
pub(super) fn parse_label(label: &str) -> (Option<String>, Option<String>) {
    if !label.starts_with("img:") && !label.starts_with("text:") {
        return (None, Some(label.to_owned()));
    }

    let mut img = None;
    let mut text = None;

    let parts: Vec<&str> = label.split(':').collect();
    let mut i = 0;

    while i < parts.len() {
        match parts.get(i) {
            Some(&"img") => {
                if i + 1 < parts.len() {
                    img = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            Some(&"text") => {
                i += 1;
                let mut text_parts = Vec::new();
                while i < parts.len() && parts[i] != "img" && parts[i] != "text" {
                    text_parts.push(parts[i]);
                    i += 1;
                }
                text = Some(text_parts.join(":").trim().to_string());
            }
            other => {
                // Treat as fallback text if no text tag is present
                if text.is_none() {
                    text = Some((*other.unwrap_or(&"")).to_string());
                } else {
                    text = Some(text.unwrap() + ":" + (*other.unwrap_or(&"")));
                }
                i += 1;
            }
        }
    }

    (img, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(img: Option<&str>, text: Option<&str>) -> (Option<String>, Option<String>) {
        (img.map(ToOwned::to_owned), text.map(ToOwned::to_owned))
    }

    #[test]
    fn test_parse_label_tagged() {
        assert_eq!(
            parse_label("img:/a.png:text:Hello"),
            label(Some("/a.png"), Some("Hello"))
        );
        assert_eq!(
            parse_label("text:a:b:img:icon"),
            label(Some("icon"), Some("a:b"))
        );
        assert_eq!(parse_label("img:icon"), label(Some("icon"), None));
        assert_eq!(
            parse_label("img:icon:Without text tag"),
            label(Some("icon"), Some("Without text tag"))
        );
    }

    #[test]
    fn test_truncate_markup() {
        assert_eq!(truncate_markup("short", 10), None);
        assert_eq!(truncate_markup("<b>short</b>", 5), None);
        assert_eq!(
            truncate_markup("<b>bold text</b> rest", 4),
            Some("<b>bold...</b>".to_owned())
        );
        assert_eq!(
            truncate_markup("a &amp; b and more", 3),
            Some("a &amp;...".to_owned())
        );
        assert_eq!(
            truncate_markup("<span color='red'>über</span>long", 5),
            Some("<span color='red'>über</span>l...".to_owned())
        );
    }

    #[test]
    fn test_parse_label_plain() {
        assert_eq!(parse_label("Firefox"), label(None, Some("Firefox")));
        assert_eq!(parse_label("img"), label(None, Some("img")));
        assert_eq!(
            parse_label("foo:text:bar"),
            label(None, Some("foo:text:bar"))
        );
        assert_eq!(
            parse_label("10:00 meeting:img:x"),
            label(None, Some("10:00 meeting:img:x"))
        );
    }
}

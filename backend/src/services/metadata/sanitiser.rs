//! Text sanitisation for metadata fields: `HTML` stripping, whitespace normalisation.
//!
//! `OPF` metadata fields arrive as untrusted strings from ingested `EPUB` files.
//! Entity and character-reference decoding is owned entirely by the `OPF`
//! parse layer (`opf_layer::read_element_text` for element text bodies, an
//! attribute-decode helper for attribute values such as `<meta content="...">`):
//! by the time a value reaches this module it is already character data, not
//! markup. Decoding it again here would reinterpret literal text a second
//! time and corrupt any legitimate entity-like content: XML 1.0 section
//! 4.4.2 treats entity inclusion as one replacement operation, not a
//! repeatable one. Without sanitisation, attacker-controlled descriptions or
//! titles containing `HTML` markup could still propagate injection vectors
//! into the rendered UI, so this module strips markup and normalises
//! whitespace before values reach the database or any downstream consumer.
//!
//! Invariant: every value returned by `sanitise` is free of `HTML` tags and
//! has whitespace collapsed to single spaces. It performs no entity decoding
//! of its own.

/// Reduce an untrusted metadata string to sanitised plaintext.
///
/// Applies the two-stage pipeline in order: `HTML` tag stripping →
/// whitespace normalisation. The pipeline mitigates `HTML` injection by
/// ensuring no markup survives into stored metadata values or the rendered
/// UI. Entity decoding is not performed here: the input is assumed to
/// already be decoded character data (see module docs).
pub fn sanitise(input: &str) -> String {
    let stripped = strip_html(input);
    normalise_whitespace(&stripped)
}

/// Strip HTML tags from a string. Simple state machine approach.
pub fn strip_html(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut in_tag = false;

    // Strip CDATA wrappers. Note: if CDATA content contains a bare '<', it will
    // activate the tag-stripping state machine and may truncate subsequent text.
    // This is acceptable — OPF descriptions rarely contain raw '<' inside CDATA.
    let input = input.replace("<![CDATA[", "").replace("]]>", "");

    for c in input.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' && in_tag {
            in_tag = false;
        } else if !in_tag {
            result.push(c);
        }
    }
    result
}

/// Normalise whitespace: collapse runs of whitespace to single space, trim.
pub fn normalise_whitespace(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut prev_ws = false;
    for c in input.chars() {
        if c.is_whitespace() {
            if !prev_ws && !result.is_empty() {
                result.push(' ');
            }
            prev_ws = true;
        } else {
            prev_ws = false;
            result.push(c);
        }
    }
    result.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_paragraph() {
        assert_eq!(strip_html("<p>Hello <em>world</em></p>"), "Hello world");
    }

    #[test]
    fn strip_html_br_variants() {
        assert_eq!(strip_html("line1<br>line2<br/>line3"), "line1line2line3");
    }

    #[test]
    fn strip_html_word_markup() {
        assert_eq!(strip_html("<o:p>text</o:p>"), "text");
    }

    #[test]
    fn strip_html_cdata() {
        assert_eq!(strip_html("<![CDATA[text]]>"), "text");
    }

    #[test]
    fn strip_html_empty() {
        assert_eq!(strip_html(""), "");
    }

    #[test]
    fn strip_html_no_tags() {
        assert_eq!(strip_html("plain text"), "plain text");
    }

    #[test]
    fn normalise_whitespace_collapse() {
        assert_eq!(normalise_whitespace("  hello   world  "), "hello world");
    }

    #[test]
    fn normalise_whitespace_newlines() {
        assert_eq!(normalise_whitespace("line1\n  \n  line2"), "line1 line2");
    }

    #[test]
    fn sanitise_full_pipeline() {
        // HTML is stripped, but the entity is left untouched: `sanitise`
        // assumes its input already arrived as decoded character data from
        // the OPF parse layer, so it must not decode entities itself.
        assert_eq!(sanitise("<p>Smith &amp; Jones</p>"), "Smith &amp; Jones");
    }

    #[test]
    fn sanitise_nested_html() {
        assert_eq!(
            sanitise("<div><p>A <strong>bold</strong> statement.</p></div>"),
            "A bold statement."
        );
    }
}

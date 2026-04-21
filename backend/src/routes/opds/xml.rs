//! XML 1.0 character sanitisation. quick-xml auto-escapes the five entities
//! (`&`, `<`, `>`, `"`, `'`) when you call `BytesText::new` or
//! `push_attribute`, but it does NOT strip codepoints outside the XML 1.0
//! `Char` production. Strict clients (Moon+ Reader, KyBook 3) reject a feed
//! containing those, so every DB-sourced string — whether rendered as a text
//! node or as an attribute value — must pass through [`sanitise_xml_text`]
//! before reaching quick-xml.

/// Strip characters NOT in the XML 1.0 `Char` production:
/// `#x9 | #xA | #xD | [#x20-#xD7FF] | [#xE000-#xFFFD] | [#x10000-#x10FFFF]`.
///
/// The 5 XML entity characters (`& < > " '`) are NOT stripped — quick-xml
/// escapes them on serialisation.
pub fn sanitise_xml_text(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            matches!(
                c,
                '\t' | '\n'
                    | '\r'
                    | '\u{20}'..='\u{D7FF}'
                    | '\u{E000}'..='\u{FFFD}'
                    | '\u{10000}'..='\u{10FFFF}'
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_control_codepoints() {
        // \x01 is outside Char; \x7F and \u{FEFF} are inside Char and kept.
        assert_eq!(sanitise_xml_text("a\x01b"), "ab");
        assert_eq!(sanitise_xml_text("a\x00b"), "ab");
        assert_eq!(sanitise_xml_text("a\x0bb"), "ab"); // vertical tab
    }

    #[test]
    fn preserves_tab_lf_cr() {
        assert_eq!(sanitise_xml_text("a\tb\nc\rd"), "a\tb\nc\rd");
    }

    #[test]
    fn preserves_emoji_and_high_codepoints() {
        assert_eq!(sanitise_xml_text("hi 😀 world"), "hi 😀 world");
        // U+1F600 is \u{1F600}
        assert_eq!(sanitise_xml_text("\u{1F600}"), "\u{1F600}");
    }

    #[test]
    fn does_not_escape_five_entity_chars() {
        // Escaping is quick-xml's job, not this function's.
        assert_eq!(sanitise_xml_text("a<b&c>d\"e'f"), "a<b&c>d\"e'f");
    }

    #[test]
    fn strips_ffff_and_fffe() {
        // XML 1.0 excludes U+FFFE and U+FFFF.
        assert_eq!(sanitise_xml_text("a\u{FFFE}b"), "ab");
        assert_eq!(sanitise_xml_text("a\u{FFFF}b"), "ab");
    }
}

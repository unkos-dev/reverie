//! XML 1.0 character sanitisation.
//!
//! quick-xml auto-escapes the five entities
//! (`&`, `<`, `>`, `"`, `'`) when you call `BytesText::new` or
//! `push_attribute`, but it does NOT strip codepoints outside the XML 1.0
//! `Char` production. Strict clients (Moon+ Reader, `KyBook` 3) reject a feed
//! containing those, so every DB-sourced string — whether rendered as a text
//! node or as an attribute value — must pass through [`sanitise_xml_text`]
//! before reaching quick-xml.

/// Strip characters NOT in the XML 1.0 `Char` production:
/// `#x9 | #xA | #xD | [#x20-#xD7FF] | [#xE000-#xFFFD] | [#x10000-#x10FFFF]`.
///
/// The 5 XML entity characters (`& < > " '`) are NOT stripped — quick-xml
/// escapes them on serialisation.
#[must_use]
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

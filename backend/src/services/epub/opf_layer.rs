//! `OPF` package-document parsing layer (Layer 3).
//!
//! Parses the `OPF` file to extract the manifest (id → href map), the reading
//! spine, Dublin Core bibliographic metadata, W3C accessibility `<meta>`
//! elements, and series metadata from both calibre (`<meta name="calibre:…">`)
//! and `EPUB3` `belongs-to-collection`. Broken spine refs (idrefs not in the
//! manifest) are removed and recorded as `Repaired` issues. Manifest hrefs
//! that fail the path-safety check are dropped and recorded as `Degraded`.
//!
//! This layer owns entity and character-reference decoding for the
//! human-readable text it emits: element text (`read_element_text`) and
//! `content` attribute values (`attr_text`). Downstream consumers, including
//! the metadata sanitiser, must treat those fields as already-decoded
//! character data and must not decode them again. The remaining attributes
//! (`id`, `idref`, `refines`, `name`, `property`, `properties`,
//! `role`/`opf:role`, `media-type`, `href`) are read raw:
//! they are reference keys compared only with each other or with literal
//! vocabulary terms, where consistent raw readings still match, and `href`
//! values in real EPUBs escape special characters with percent-encoding,
//! not entity references. The known limitation is that an entity-escaped
//! `href` stays raw and will not resolve to its archive entry.

use quick_xml::Reader;
use quick_xml::escape::resolve_xml_entity;
use quick_xml::events::Event;
use quick_xml::events::attributes::Attribute;
use quick_xml::name::QName;
use quick_xml::{Decoder, XmlVersion};
use std::collections::{HashMap, HashSet};

use super::{
    Issue, IssueKind, Layer, Severity,
    zip_layer::{ZipHandle, read_entry},
};

/// A Dublin Core `dc:creator` or `dc:contributor` entry with any declared `MARC` relator roles.
#[derive(Debug, Clone)]
pub struct Creator {
    /// The creator's name as it appears in the `OPF` metadata.
    pub name: String,
    /// `MARC` relator codes in document order, from the `opf:role`/`role`
    /// attribute (`EPUB2`) and/or `<meta refines="#id" property="role">`
    /// (`EPUB3`, where a single creator may carry more than one role refine).
    /// Empty when no role was declared.
    pub roles: Vec<String>,
    /// `true` for `dc:contributor`, `false` for `dc:creator`.
    pub from_contributor: bool,
}

/// Parse-time buffer for a `dc:creator`/`dc:contributor` element, held until
/// role refines (which may appear anywhere in the document) are resolved.
struct CreatorBuf {
    id: Option<String>,
    name: String,
    inline_role: Option<String>,
    from_contributor: bool,
}

/// Series affiliation metadata sourced from calibre or `EPUB3` collection elements.
#[derive(Debug, Clone)]
pub struct SeriesMeta {
    /// Series title (calibre `calibre:series` or `EPUB3` `belongs-to-collection`).
    pub name: String,
    /// Position within the series, if declared.
    pub position: Option<f64>,
}

/// All data extracted from the `OPF` package document in a single parse pass.
#[derive(Debug, Clone)]
pub struct OpfData {
    /// All manifest items: id → href
    pub manifest: HashMap<String, String>,
    /// Href of the manifest item declared as the cover via the EPUB 3
    /// `properties="cover-image"` attribute, if any. Captured independently of
    /// the item's `id` — Standard Ebooks declare `id="cover.svg"` (not a magic
    /// id), so id-only detection misses them.
    pub cover_href: Option<String>,
    /// Href of the manifest item referenced by the EPUB 2
    /// `<meta name="cover" content="ID"/>` declaration, if any, resolved via
    /// the manifest id lookup. The referenced `ID` is an author-chosen
    /// manifest item id and is not necessarily one of the legacy magic ids
    /// (`cover-image`, `cover`, ...), so this must be resolved independently
    /// of the id heuristic. Only resolves when the referenced item's
    /// `media-type` is an `image` type (matched case-insensitively per MIME
    /// rules) -- real EPUB2s routinely point this
    /// meta at the XHTML cover page rather than the image, and resolving that
    /// would shadow the magic-id heuristic that would otherwise find the real
    /// cover. `None` when no such meta exists, its `content` does not resolve
    /// to a manifest item, or the resolved item is not an image.
    pub meta_cover_href: Option<String>,
    /// Spine idrefs (after removing broken refs)
    pub spine_idrefs: Vec<String>,
    /// `OPF` path within the archive (needed by repair and other layers)
    pub opf_path: String,
    /// Raw W3C accessibility metadata from `<meta>` elements, if any
    pub accessibility_metadata: Option<serde_json::Value>,
    /// Dublin Core: title (first `dc:title` without a `title-type=subtitle` refine)
    pub title: Option<String>,
    /// Declared subtitle: text of the first `dc:title` refined `title-type="subtitle"`.
    /// `None` when no such refine exists (no colon-split heuristics are applied).
    pub subtitle: Option<String>,
    /// Dublin Core: creators and contributors with any declared roles
    pub creators: Vec<Creator>,
    /// Raw `schema:numberOfPages` meta text, if present. A community convention,
    /// not part of the `EPUB 3.3` core vocabulary; parsed to an integer downstream.
    pub number_of_pages: Option<String>,
    /// Dublin Core: description (may contain HTML)
    pub description: Option<String>,
    /// Dublin Core: publisher
    pub publisher: Option<String>,
    /// Dublin Core: date (raw string)
    pub date: Option<String>,
    /// Dublin Core: language
    pub language: Option<String>,
    /// Dublin Core: all identifier values (ISBNs, URNs, etc.)
    pub identifiers: Vec<String>,
    /// Dublin Core: subject values
    pub subjects: Vec<String>,
    /// Series metadata (calibre or `EPUB3` collection)
    pub series_meta: Option<SeriesMeta>,
}

/// Extract the local name from a possibly-namespaced element name.
/// e.g. b"dc:title" → b"title", b"title" → b"title"
fn local_name(name: &[u8]) -> &[u8] {
    name.iter()
        .position(|&b| b == b':')
        .map_or(name, |pos| &name[pos + 1..])
}

/// Resolve a named entity reference in `OPF` metadata text.
///
/// The five predefined XML entities plus `nbsp`, the one HTML entity this
/// pipeline has always resolved: `OPF` metadata is routinely authored by
/// HTML-oriented tooling that emits a singly-escaped `&nbsp;` in titles and
/// descriptions, and leaving it raw would surface literal markup in the UI
/// and re-escape it to `&amp;nbsp;` in the source file on writeback.
/// Anything else is unresolvable without a `DTD` and is kept as literal
/// markup by callers rather than guessed at.
fn resolve_metadata_entity(name: &str) -> Option<&'static str> {
    match name {
        "nbsp" => Some("\u{00A0}"),
        _ => resolve_xml_entity(name),
    }
}

/// Parse the numeric body of a character reference (`233` or `x1F`) to a
/// character under the same rules as the strict parser: lowercase `x` for
/// hex, no sign, no `NUL`, and the code point must be a valid `char`.
fn parse_char_ref(num: &str) -> Option<char> {
    let (digits, radix) = num.strip_prefix('x').map_or((num, 10), |hex| (hex, 16));
    // `from_str_radix` accepts a leading `+`, which XML does not.
    if digits.starts_with('+') {
        return None;
    }
    let code = u32::from_str_radix(digits, radix).ok()?;
    if code == 0 {
        return None;
    }
    char::from_u32(code)
}

/// Resolve one reference body (the text between `&` and `;`) to its
/// replacement text, or `None` when it is not resolvable.
fn resolve_reference(body: &str) -> Option<String> {
    body.strip_prefix('#').map_or_else(
        || resolve_metadata_entity(body).map(String::from),
        |num| parse_char_ref(num).map(String::from),
    )
}

/// Best-effort decode of an attribute value that failed strict decoding.
///
/// Mirrors `read_element_text`'s treatment of references one at a time:
/// resolvable character and named references decode, and an unresolvable or
/// malformed one stays as literal markup without disturbing its neighbours.
/// Literal whitespace is normalised to spaces first (XML attribute-value
/// normalisation), before references are expanded, so whitespace produced
/// by a character reference such as `&#x9;` survives as itself, matching
/// the strict path's ordering.
fn lenient_attr_text(decoder: Decoder, a: &Attribute<'_>) -> Option<String> {
    let raw = decoder.decode(&a.value).ok()?;

    let mut normalized = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                normalized.push(' ');
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
            }
            '\n' | '\t' => normalized.push(' '),
            _ => normalized.push(c),
        }
    }

    let mut result = String::with_capacity(normalized.len());
    let mut remaining = normalized.as_str();
    while let Some(amp) = remaining.find('&') {
        result.push_str(&remaining[..amp]);
        remaining = &remaining[amp..];
        // Lookahead capped at 12 bytes: longer than any resolvable
        // reference (`&#x10FFFF;` is 10 characters), so one missing
        // semicolon does not pull the rest of the value into the scan.
        let lookahead = remaining.len().min(12);
        let semi = remaining.as_bytes()[..lookahead]
            .iter()
            .position(|&b| b == b';');
        let resolved =
            semi.and_then(|semi| resolve_reference(&remaining[1..semi]).map(|r| (r, semi)));
        if let Some((replacement, semi)) = resolved {
            result.push_str(&replacement);
            remaining = &remaining[semi + 1..];
        } else {
            result.push('&');
            remaining = &remaining[1..];
        }
    }
    result.push_str(remaining);
    Some(result)
}

/// Decode an attribute value as character data.
///
/// Attribute values are never auto-unescaped by `quick-xml`, unlike element
/// text (see `read_element_text`): `a.value` is the raw markup between the
/// quotes. Predefined entities (plus `nbsp`) and numeric character
/// references are resolved here so an attribute-sourced field (for example
/// `<meta name="calibre:series" content="...">`) reaches the same decoded
/// state as an element-text-sourced one, regardless of which XML construct
/// carried it. When strict decoding fails (an unrecognised named entity, a
/// bare `&`, or an illegal character reference), the value is re-decoded
/// leniently instead: each unresolvable reference is kept as literal markup
/// while every resolvable one around it still decodes, matching
/// `read_element_text`'s per-reference degradation. Real-world `OPF`
/// attribute values are not always strictly valid XML, and an
/// all-or-nothing fallback would let a single bad reference turn every
/// `&amp;` in the value back into literal markup.
fn attr_text(decoder: Decoder, a: &Attribute<'_>) -> Option<String> {
    a.decoded_and_normalized_value_with(
        XmlVersion::Implicit1_0,
        decoder,
        1,
        resolve_metadata_entity,
    )
    .map_or_else(|_| lenient_attr_text(decoder, a), |v| Some(v.into_owned()))
}

/// Read an element's character data up to its matching end tag.
///
/// `Reader::read_text` returns the raw markup span between the tags rather
/// than decoding it, which is wrong for two different reasons at once: a
/// `CDATA` section comes back as literal `<![CDATA[...]]>` markup instead of
/// its content, while a text node's entities (`&amp;`) come back unresolved.
/// The two need opposite treatment, not one shared pass over the raw span:
/// `CDATA` content is never escaped (`<![CDATA[&amp;]]>` is the five literal
/// characters `&amp;`), but a text node's entities must be resolved (`a
/// &amp; b` is the string `a & b`). Walking events keeps that distinction:
/// `Text` is decoded and its split-out `GeneralRef` entities resolved,
/// `CData` is taken verbatim, and the parts are concatenated in document
/// order, which also naturally handles a body mixing both forms (legal XML).
fn read_element_text(reader: &mut Reader<&[u8]>, end: QName) -> Option<String> {
    // `trim_text` (enabled document-wide, so that insignificant whitespace
    // between sibling elements is ignored elsewhere) trims each `Text` event
    // independently. That is wrong here: it would eat the spaces on either
    // side of a `CData` section or a resolved entity, since those split a
    // single logical body into more than one `Text` event. Disable it for
    // the span this call reads and restore it before returning, since the
    // caller's final `.trim()` on the assembled string already handles
    // trimming the body's outer edges.
    let config = reader.config_mut();
    let trim_start = config.trim_text_start;
    let trim_end = config.trim_text_end;
    config.trim_text_start = false;
    config.trim_text_end = false;

    let mut text = String::new();
    let mut depth = 0u32;
    let result = 'parse: loop {
        let Ok(event) = reader.read_event() else {
            break 'parse None;
        };
        match event {
            Event::Start(s) if s.name().as_ref() == end.as_ref() => depth += 1,
            Event::End(e) if e.name().as_ref() == end.as_ref() => {
                if depth == 0 {
                    break 'parse Some(text);
                }
                depth -= 1;
            }
            Event::Text(t) => match t.decode() {
                Ok(s) => text.push_str(&s),
                Err(_) => break 'parse None,
            },
            Event::CData(c) => match c.decode() {
                Ok(s) => text.push_str(&s),
                Err(_) => break 'parse None,
            },
            Event::GeneralRef(r) => match r.resolve_char_ref() {
                Ok(Some(ch)) => text.push(ch),
                Ok(None) => {
                    let Ok(name) = r.decode() else {
                        break 'parse None;
                    };
                    if let Some(resolved) = resolve_metadata_entity(&name) {
                        text.push_str(resolved);
                    } else {
                        // Not a resolvable named entity (the predefined
                        // set plus nbsp) and not a character reference;
                        // without a DTD there is nothing to resolve it
                        // against, so keep the original markup rather
                        // than lose it.
                        text.push('&');
                        text.push_str(&name);
                        text.push(';');
                    }
                }
                Err(_) => {
                    // The reference was syntactically valid but its code
                    // point is illegal XML (a surrogate, NUL, or
                    // out-of-range value), so `resolve_char_ref` errors.
                    // Treat this exactly like the unresolvable named-entity
                    // case above rather than aborting: bailing here would
                    // leave the reader positioned mid-element, and the
                    // caller's loop would misread the remainder of this
                    // element's body as document-level content.
                    let Ok(name) = r.decode() else {
                        break 'parse None;
                    };
                    text.push('&');
                    text.push_str(&name);
                    text.push(';');
                }
            },
            Event::Eof => break 'parse None,
            _ => {}
        }
    };

    let config = reader.config_mut();
    config.trim_text_start = trim_start;
    config.trim_text_end = trim_end;

    result
}

/// Parse the `OPF` package document at `opf_path` and return structured metadata.
///
/// Extracts the manifest, spine, Dublin Core fields, W3C accessibility `<meta>`
/// elements, and series metadata in a single `quick_xml` pass. Broken spine
/// idrefs (not present in the manifest) are removed and recorded as `Repaired`
/// issues. Manifest hrefs failing the path-safety check are dropped with a
/// `Degraded` issue.
///
/// Returns `None` if `opf_path` is `None`, the entry cannot be read from
/// `handle`, or the entry bytes are not valid `UTF-8`.
#[expect(
    clippy::too_many_lines,
    reason = "OPF parser handles the full EPUB 2/3 metadata element set in one pass; the per-element cases are mechanical and cannot meaningfully be split without introducing a second parse pass"
)]
pub fn validate(
    handle: &ZipHandle,
    opf_path: Option<&str>,
    issues: &mut Vec<Issue>,
) -> Option<OpfData> {
    let path = opf_path?;
    let bytes = read_entry(handle, path)?;
    let xml = std::str::from_utf8(&bytes).ok()?;

    let mut manifest: HashMap<String, String> = HashMap::new();
    // id -> media-type, kept alongside `manifest` only to gate EPUB 2 meta
    // cover resolution below (see `meta_cover_href` resolution): the public
    // `OpfData.manifest` shape stays id -> href for every other consumer.
    let mut manifest_media_types: HashMap<String, String> = HashMap::new();
    let mut cover_href: Option<String> = None;
    // Manifest item id referenced by an EPUB 2 <meta name="cover"
    // content="ID"/>, buffered raw and resolved against `manifest` after the
    // parse pass (metadata precedes manifest in a well-formed OPF, but the
    // buffer-and-resolve pattern makes resolution independent of order,
    // consistent with group_positions/role_refines below).
    let mut meta_cover_id: Option<String> = None;
    let mut spine_idrefs: Vec<String> = Vec::new();
    let mut accessibility_meta: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    // Dublin Core fields.
    // Titles and creators are buffered with their `id` attribute (when
    // present) so EPUB3 `<meta refines="#id" ...>` role/title-type refines
    // can be resolved against them after the pass, independent of whether
    // the refine appears before or after the element it targets.
    let mut title_elements: Vec<(Option<String>, String)> = Vec::new();
    let mut creator_bufs: Vec<CreatorBuf> = Vec::new();
    let mut description: Option<String> = None;
    let mut publisher: Option<String> = None;
    let mut date: Option<String> = None;
    let mut language: Option<String> = None;
    let mut identifiers: Vec<String> = Vec::new();
    let mut subjects: Vec<String> = Vec::new();
    let mut number_of_pages: Option<String> = None;

    // Series metadata (calibre or EPUB 3).
    // EPUB 3 group-position is matched to its collection by `refines` target,
    // independent of document order: positions are buffered into a map keyed
    // by the refines target (`#<collection-id>`) as they are seen, then joined
    // to the collection's id at the end of the pass.
    let mut calibre_series_name: Option<String> = None;
    let mut calibre_series_index: Option<f64> = None;
    let mut epub3_collection_name: Option<String> = None;
    let mut epub3_collection_id: Option<String> = None;
    let mut group_positions: HashMap<String, f64> = HashMap::new();
    // EPUB3 role/title-type refines, keyed by their `refines` target id
    // (buffered the same way as group_positions, for the same reason).
    let mut role_refines: HashMap<String, Vec<String>> = HashMap::new();
    let mut subtitle_ids: HashSet<String> = HashSet::new();

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event().ok()? {
            // EPUB 3 text-content meta: <meta property="schema:accessMode">textual</meta>
            // Also handles belongs-to-collection and group-position.
            // Must come BEFORE general Event::Start arm to avoid shadowing.
            Event::Start(e) if e.name().as_ref() == b"meta" => {
                let e = e.into_owned(); // release reader buffer borrow before read_element_text
                let prop = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"property")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });
                let content_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"content")
                    .and_then(|a| attr_text(reader.decoder(), &a));
                let id_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"id")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });
                let refines_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"refines")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });
                let name_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"name")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });

                // EPUB 2 cover declaration in Start-tag form:
                // <meta name="cover" content="ID"></meta>. First match wins,
                // mirroring the cover_href property-item resolution below.
                if meta_cover_id.is_none()
                    && name_attr.as_deref() == Some("cover")
                    && let Some(ref id) = content_attr
                {
                    meta_cover_id = Some(id.clone());
                }

                if let Some(ref prop) = prop {
                    if prop == "belongs-to-collection" {
                        let text = read_element_text(&mut reader, e.name())
                            .map(|t| t.trim().to_string())
                            .filter(|s| !s.is_empty());
                        if let Some(name) = text {
                            epub3_collection_name = Some(name);
                            epub3_collection_id = id_attr;
                        }
                        continue;
                    }
                    if prop == "group-position" {
                        // Buffer the position keyed by its refines target,
                        // regardless of whether the collection has been seen
                        // yet — resolved against the collection id at end of
                        // pass so element order doesn't matter.
                        if let Some(refines) = refines_attr {
                            let text = content_attr.or_else(|| {
                                read_element_text(&mut reader, e.name())
                                    .map(|t| t.trim().to_string())
                                    .filter(|s| !s.is_empty())
                            });
                            if let Some(pos) = text.and_then(|t| t.parse::<f64>().ok()) {
                                group_positions.insert(refines, pos);
                            }
                        }
                        continue;
                    }
                    if prop == "role" {
                        // Multiple role refines may target the same creator
                        // (EPUB 3.3 §D.3.10 example: one creator, aut + ill).
                        if let Some(refines) = refines_attr {
                            let text = content_attr.or_else(|| {
                                read_element_text(&mut reader, e.name())
                                    .map(|t| t.trim().to_string())
                                    .filter(|s| !s.is_empty())
                            });
                            if let Some(code) = text {
                                role_refines.entry(refines).or_default().push(code);
                            }
                        }
                        continue;
                    }
                    if prop == "title-type" {
                        if let Some(refines) = refines_attr {
                            let text = content_attr.or_else(|| {
                                read_element_text(&mut reader, e.name())
                                    .map(|t| t.trim().to_string())
                                    .filter(|s| !s.is_empty())
                            });
                            if text.as_deref() == Some("subtitle") {
                                subtitle_ids.insert(refines);
                            }
                        }
                        continue;
                    }
                    if prop == "schema:numberOfPages" {
                        let text = content_attr.or_else(|| {
                            read_element_text(&mut reader, e.name())
                                .map(|t| t.trim().to_string())
                                .filter(|s| !s.is_empty())
                        });
                        if let Some(n) = text {
                            number_of_pages = Some(n);
                        }
                        continue;
                    }
                    if prop.starts_with("schema:access") || prop.starts_with("dcterms:") {
                        let val = content_attr.or_else(|| {
                            read_element_text(&mut reader, e.name())
                                .map(|t| t.trim().to_string())
                                .filter(|s| !s.is_empty())
                        });
                        if let Some(v) = val {
                            accessibility_meta.insert(prop.clone(), serde_json::Value::String(v));
                        }
                    }
                }
            }
            // EPUB 2 attribute-style meta: <meta name="..." content="..."/>
            Event::Empty(e) if e.name().as_ref() == b"meta" => {
                let prop = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"property")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });
                let name_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"name")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });
                let content = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"content")
                    .and_then(|a| attr_text(reader.decoder(), &a));
                let refines_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"refines")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });

                // EPUB 3 self-closing group-position: value lives in the
                // `content` attr, no text body (quick-xml fires Event::Empty,
                // not Event::Start). Mirrors the Event::Start arm's
                // buffer-and-resolve pattern, extended to cover the
                // self-closing form the Event::Start arm alone can't reach.
                if let Some(ref prop) = prop
                    && prop == "group-position"
                {
                    if let Some(refines) = refines_attr.clone()
                        && let Some(pos) = content.as_deref().and_then(|c| c.parse::<f64>().ok())
                    {
                        group_positions.insert(refines, pos);
                    }
                    continue;
                }

                // Self-closing role/title-type/numberOfPages refines: same
                // buffer-and-resolve pattern as group-position above.
                if let Some(ref prop) = prop
                    && prop == "role"
                {
                    if let Some(refines) = refines_attr.clone()
                        && let Some(code) = content.clone()
                    {
                        role_refines.entry(refines).or_default().push(code);
                    }
                    continue;
                }
                if let Some(ref prop) = prop
                    && prop == "title-type"
                {
                    if let Some(refines) = refines_attr.clone()
                        && content.as_deref() == Some("subtitle")
                    {
                        subtitle_ids.insert(refines);
                    }
                    continue;
                }
                if let Some(ref prop) = prop
                    && prop == "schema:numberOfPages"
                {
                    if let Some(n) = content.clone() {
                        number_of_pages = Some(n);
                    }
                    continue;
                }

                // Accessibility meta via property attribute
                if let Some(ref prop) = prop
                    && (prop.starts_with("schema:access") || prop.starts_with("dcterms:"))
                    && let Some(ref v) = content
                {
                    accessibility_meta.insert(prop.clone(), serde_json::Value::String(v.clone()));
                }

                // Calibre series meta via name attribute
                if let Some(ref name) = name_attr
                    && let Some(ref c) = content
                {
                    match name.as_str() {
                        "calibre:series" => calibre_series_name = Some(c.clone()),
                        "calibre:series_index" => calibre_series_index = c.parse::<f64>().ok(),
                        // EPUB 2 cover declaration: <meta name="cover"
                        // content="ID"/>, the common self-closing form. First
                        // match wins, mirroring the Start-tag arm above.
                        "cover" if meta_cover_id.is_none() => meta_cover_id = Some(c.clone()),
                        _ => {}
                    }
                }
            }
            // Dublin Core elements: <dc:title>, <dc:creator>, <dc:contributor>, etc.
            Event::Start(e)
                if matches!(
                    local_name(e.name().as_ref()),
                    b"title"
                        | b"creator"
                        | b"contributor"
                        | b"description"
                        | b"publisher"
                        | b"date"
                        | b"language"
                        | b"identifier"
                        | b"subject"
                ) && e.name().as_ref() != b"meta" =>
            {
                let local = local_name(e.name().as_ref()).to_vec();
                // opf:role/role attribute: EPUB2 relator code. Kept as a plain,
                // non-namespace-aware attribute match (not `NsReader`) because
                // real-world EPUB2 files routinely omit the `xmlns:opf`
                // declaration despite the spec MUST; strict namespace
                // resolution would silently drop the attribute. quick-xml
                // 0.41 also removed `.resolve_attribute()`, so there is no
                // drop-in namespace-aware alternative to reach for here.
                let inline_role = e
                    .attributes()
                    .flatten()
                    .find(|a| {
                        let k = a.key.as_ref();
                        k == b"opf:role" || k == b"role"
                    })
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });
                let id_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"id")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });

                let e = e.into_owned();
                let text = read_element_text(&mut reader, e.name())
                    .map(|t| t.trim().to_string())
                    .filter(|s| !s.is_empty());

                if let Some(text) = text {
                    match local.as_slice() {
                        b"title" => title_elements.push((id_attr, text)),
                        b"creator" => creator_bufs.push(CreatorBuf {
                            id: id_attr,
                            name: text,
                            inline_role,
                            from_contributor: false,
                        }),
                        b"contributor" => creator_bufs.push(CreatorBuf {
                            id: id_attr,
                            name: text,
                            inline_role,
                            from_contributor: true,
                        }),
                        b"description" if description.is_none() => description = Some(text),
                        b"publisher" if publisher.is_none() => publisher = Some(text),
                        b"date" if date.is_none() => date = Some(text),
                        b"language" if language.is_none() => language = Some(text),
                        b"identifier" => identifiers.push(text),
                        b"subject" => subjects.push(text),
                        _ => {}
                    }
                }
            }
            // General arm — meta and DC already handled by guarded arms above
            Event::Empty(e) | Event::Start(e) => match e.name().as_ref() {
                b"item" => {
                    let attrs: HashMap<String, String> = e
                        .attributes()
                        .flatten()
                        .filter_map(|a| {
                            let k = std::str::from_utf8(a.key.as_ref()).ok()?.to_string();
                            let v = std::str::from_utf8(&a.value).ok()?.to_string();
                            Some((k, v))
                        })
                        .collect();

                    if let (Some(id), Some(href)) = (attrs.get("id"), attrs.get("href")) {
                        // C4: validate href path safety via shared helper.
                        if super::is_safe_path(href) {
                            manifest.insert(id.clone(), href.clone());
                            if let Some(media_type) = attrs.get("media-type") {
                                manifest_media_types.insert(id.clone(), media_type.clone());
                            }
                            // EPUB 3 cover detection: the cover is the item
                            // carrying `properties="cover-image"`, regardless of
                            // its id. `properties` is a space-separated token
                            // list. First match wins.
                            if cover_href.is_none()
                                && attrs.get("properties").is_some_and(|p| {
                                    p.split_ascii_whitespace().any(|t| t == "cover-image")
                                })
                            {
                                cover_href = Some(href.clone());
                            }
                        } else {
                            issues.push(Issue {
                                layer: Layer::Opf,
                                severity: Severity::Degraded,
                                kind: IssueKind::UnsafeManifestHref { href: href.clone() },
                            });
                        }
                    }
                }
                b"itemref" => {
                    if let Some(idref) = e
                        .attributes()
                        .flatten()
                        .find(|a| a.key.as_ref() == b"idref")
                        && let Ok(v) = std::str::from_utf8(&idref.value)
                    {
                        spine_idrefs.push(v.to_string());
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }

    // Resolve the EPUB 2 meta-declared cover id against the manifest now that
    // the full pass has completed. Real EPUB2s routinely point <meta
    // name="cover"> at the XHTML cover PAGE while the actual image sits
    // under a magic id (e.g. "cover-image") -- gate on media-type so a
    // non-image target leaves this None and lets the id heuristic run
    // instead of resolving to an undecodable page. Mirrors calibre's own
    // cover-detection behavior.
    let meta_cover_href = meta_cover_id.and_then(|id| {
        let href = manifest.get(&id)?;
        let media_type = manifest_media_types.get(&id)?;
        // MIME type matching is case-insensitive (RFC 2045), so IMAGE/JPEG
        // is as valid as image/jpeg.
        let top_level = media_type.split('/').next().unwrap_or("");
        top_level
            .eq_ignore_ascii_case("image")
            .then(|| href.clone())
    });

    // Validate spine refs against manifest
    let manifest_ids: HashSet<&String> = manifest.keys().collect();
    let mut valid_spine: Vec<String> = Vec::new();
    for idref in &spine_idrefs {
        if manifest_ids.contains(idref) {
            valid_spine.push(idref.clone());
        } else {
            issues.push(Issue {
                layer: Layer::Opf,
                severity: Severity::Repaired,
                kind: IssueKind::BrokenSpineRef {
                    idref: idref.clone(),
                },
            });
        }
    }

    let accessibility_metadata = if accessibility_meta.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(accessibility_meta))
    };

    // Resolve series: prefer calibre (more common), fall back to EPUB 3 collection
    let series_meta = calibre_series_name
        .map(|name| SeriesMeta {
            name,
            position: calibre_series_index,
        })
        .or_else(|| {
            epub3_collection_name.map(|name| {
                let position = epub3_collection_id
                    .as_ref()
                    .and_then(|id| group_positions.get(&format!("#{id}")).copied());
                SeriesMeta { name, position }
            })
        });

    // Main title = first dc:title without a subtitle refine; subtitle = text
    // of the first one with it. Creator precedence is document order per
    // EPUB 3.3 §D.3.5 (display-seq is deliberately not consulted), so when no
    // refines exist at all the first title wins and subtitle stays None,
    // matching pre-EPUB3 behavior exactly.
    let mut title: Option<String> = None;
    let mut subtitle: Option<String> = None;
    for (id, text) in title_elements {
        let is_subtitle = id
            .as_ref()
            .is_some_and(|i| subtitle_ids.contains(&format!("#{i}")));
        if is_subtitle {
            if subtitle.is_none() {
                subtitle = Some(text);
            }
        } else if title.is_none() {
            title = Some(text);
        }
    }

    // Resolve each creator's roles: inline opf:role (EPUB2) plus any EPUB3
    // role refines targeting its id, in that order.
    let creators: Vec<Creator> = creator_bufs
        .into_iter()
        .map(|cb| {
            let mut roles: Vec<String> = Vec::new();
            if let Some(r) = cb.inline_role {
                roles.push(r);
            }
            if let Some(refined) = cb
                .id
                .as_ref()
                .and_then(|id| role_refines.get(&format!("#{id}")))
            {
                roles.extend(refined.iter().cloned());
            }
            Creator {
                name: cb.name,
                roles,
                from_contributor: cb.from_contributor,
            }
        })
        .collect();

    Some(OpfData {
        manifest,
        cover_href,
        meta_cover_href,
        spine_idrefs: valid_spine,
        opf_path: path.to_string(),
        accessibility_metadata,
        title,
        subtitle,
        creators,
        number_of_pages,
        description,
        publisher,
        date,
        language,
        identifiers,
        subjects,
        series_meta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::epub::zip_layer::ZipHandle;

    fn make_handle(opf_content: &[u8]) -> ZipHandle {
        use std::io::Write;
        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);
        let opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
            zip::write::FileOptions::default();
        w.start_file("OEBPS/content.opf", opts).unwrap();
        w.write_all(opf_content).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        ZipHandle {
            bytes,
            entries: vec!["OEBPS/content.opf".to_string()],
        }
    }

    #[test]
    fn broken_spine_ref_emits_repaired_issue() {
        let opf = br#"<package>
            <manifest>
                <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
            </manifest>
            <spine>
                <itemref idref="ch1"/>
                <itemref idref="ch2"/>
            </spine>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        assert!(result.is_some());
        let data = result.unwrap();
        assert_eq!(data.spine_idrefs, vec!["ch1"]);
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Repaired
                && matches!(&i.kind, IssueKind::BrokenSpineRef { idref } if idref == "ch2")
        }));
    }

    #[test]
    fn epub3_properties_cover_image_detected() {
        // Standard Ebooks shape: the cover is declared via
        // properties="cover-image" with id "cover.svg" — NOT one of the legacy
        // magic ids, so id-only detection misses it.
        let opf = br#"<package>
            <manifest>
                <item id="cover.svg" href="images/cover.svg" media-type="image/svg+xml" properties="cover-image"/>
                <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
            </manifest>
            <spine><itemref idref="ch1"/></spine>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.cover_href.as_deref(), Some("images/cover.svg"));
    }

    #[test]
    fn no_cover_image_property_leaves_cover_href_none() {
        let opf = br#"<package>
            <manifest>
                <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
            </manifest>
            <spine><itemref idref="ch1"/></spine>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert!(data.cover_href.is_none());
    }

    #[test]
    fn epub2_meta_cover_self_closing_resolves_via_manifest_id() {
        // EPUB 2 declaration: <meta name="cover" content="ID"/> is the common
        // self-closing form (no text body -> quick-xml yields Event::Empty).
        // The referenced id ("cvr") is not one of the legacy magic ids, so
        // only the meta lookup can resolve it.
        let opf = br#"<package>
            <metadata>
                <meta name="cover" content="cvr"/>
            </metadata>
            <manifest>
                <item id="cvr" href="images/cover.jpg" media-type="image/jpeg"/>
                <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
            </manifest>
            <spine><itemref idref="ch1"/></spine>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.meta_cover_href.as_deref(), Some("images/cover.jpg"));
    }

    #[test]
    fn epub2_meta_cover_uppercase_media_type_resolves() {
        // MIME type matching is case-insensitive (RFC 2045); IMAGE/JPEG is
        // as valid as image/jpeg and must pass the image gate.
        let opf = br#"<package>
            <metadata>
                <meta name="cover" content="cvr"/>
            </metadata>
            <manifest>
                <item id="cvr" href="images/cover.jpg" media-type="IMAGE/JPEG"/>
                <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
            </manifest>
            <spine><itemref idref="ch1"/></spine>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.meta_cover_href.as_deref(), Some("images/cover.jpg"));
    }

    #[test]
    fn epub2_meta_cover_start_tag_form_resolves_via_manifest_id() {
        // Non-self-closing form emitted by some toolchains: quick-xml yields
        // Event::Start/Event::End rather than Event::Empty.
        let opf = br#"<package>
            <metadata>
                <meta name="cover" content="cvr"></meta>
            </metadata>
            <manifest>
                <item id="cvr" href="images/cover.jpg" media-type="image/jpeg"/>
                <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
            </manifest>
            <spine><itemref idref="ch1"/></spine>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.meta_cover_href.as_deref(), Some("images/cover.jpg"));
    }

    #[test]
    fn meta_cover_pointing_to_non_image_item_falls_through_to_magic_id() {
        // Real-world EPUB 2 shape: <meta name="cover"> targets the XHTML
        // cover PAGE, not the image, while the actual cover image sits under
        // the magic id `cover-image`. The media-type gate must reject the
        // XHTML target so `meta_cover_href` stays None and the id heuristic
        // still finds the real image (calibre applies the same gate).
        let opf = br#"<package>
            <metadata>
                <meta name="cover" content="titlepage"/>
            </metadata>
            <manifest>
                <item id="titlepage" href="titlepage.xhtml" media-type="application/xhtml+xml"/>
                <item id="cover-image" href="images/cover.jpg" media-type="image/jpeg"/>
            </manifest>
            <spine><itemref idref="titlepage"/></spine>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert!(data.meta_cover_href.is_none());
        assert_eq!(
            crate::services::epub::cover_layer::find_cover_href(&data).as_deref(),
            Some("images/cover.jpg")
        );
    }

    #[test]
    fn epub2_meta_cover_with_unknown_id_leaves_meta_cover_href_none() {
        // The meta points at an id that isn't in the manifest at all -- must
        // not resolve to a bogus href.
        let opf = br#"<package>
            <metadata>
                <meta name="cover" content="does-not-exist"/>
            </metadata>
            <manifest>
                <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
            </manifest>
            <spine><itemref idref="ch1"/></spine>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert!(data.meta_cover_href.is_none());
    }

    #[test]
    fn epub3_accessibility_meta_parsed() {
        let opf = br#"<package>
            <metadata>
                <meta property="schema:accessMode">textual</meta>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        assert!(result.is_some());
        let data = result.unwrap();
        let meta = data.accessibility_metadata.unwrap();
        assert_eq!(meta["schema:accessMode"], "textual");
    }

    #[test]
    fn dc_metadata_extracted() {
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:title>The Hobbit</dc:title>
                <dc:creator opf:role="aut">J. R. R. Tolkien</dc:creator>
                <dc:description>A fantasy novel</dc:description>
                <dc:publisher>Allen &amp; Unwin</dc:publisher>
                <dc:date>1937-09-21</dc:date>
                <dc:language>en</dc:language>
                <dc:identifier>urn:isbn:9780547928227</dc:identifier>
                <dc:identifier>urn:uuid:12345</dc:identifier>
                <dc:subject>Fantasy</dc:subject>
                <dc:subject>Adventure</dc:subject>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        assert_eq!(data.title.as_deref(), Some("The Hobbit"));
        assert_eq!(data.creators.len(), 1);
        assert_eq!(data.creators[0].name, "J. R. R. Tolkien");
        assert_eq!(data.creators[0].roles, vec!["aut"]);
        assert_eq!(data.description.as_deref(), Some("A fantasy novel"));
        assert_eq!(data.publisher.as_deref(), Some("Allen & Unwin"));
        assert_eq!(data.date.as_deref(), Some("1937-09-21"));
        assert_eq!(data.language.as_deref(), Some("en"));
        assert_eq!(data.identifiers.len(), 2);
        assert_eq!(data.subjects, vec!["Fantasy", "Adventure"]);
    }

    #[test]
    fn calibre_series_meta_extracted() {
        let opf = br#"<package>
            <metadata>
                <dc:title>The Two Towers</dc:title>
                <meta name="calibre:series" content="The Lord of the Rings"/>
                <meta name="calibre:series_index" content="2"/>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        let series = data.series_meta.unwrap();
        assert_eq!(series.name, "The Lord of the Rings");
        assert_eq!(series.position, Some(2.0));
    }

    #[test]
    fn epub3_collection_series_extracted() {
        let opf = br##"<package>
            <metadata>
                <dc:title>A Game of Thrones</dc:title>
                <meta property="belongs-to-collection" id="c01">A Song of Ice and Fire</meta>
                <meta refines="#c01" property="group-position">1</meta>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        let series = data.series_meta.unwrap();
        assert_eq!(series.name, "A Song of Ice and Fire");
        assert_eq!(series.position, Some(1.0));
    }

    #[test]
    fn epub3_collection_series_reversed_order() {
        // group-position appears BEFORE belongs-to-collection in document
        // order — valid per EPUB3, emitted by some toolchains. Position must
        // still be captured.
        let opf = br##"<package>
            <metadata>
                <dc:title>A Clash of Kings</dc:title>
                <meta refines="#c01" property="group-position">3</meta>
                <meta property="belongs-to-collection" id="c01">A Song of Ice and Fire</meta>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        let series = data.series_meta.unwrap();
        assert_eq!(series.name, "A Song of Ice and Fire");
        assert_eq!(series.position, Some(3.0));
    }

    #[test]
    fn epub3_collection_series_self_closing_position() {
        // Self-closing group-position carries its value in a `content` attr
        // (no text body → quick-xml yields Event::Empty, not Event::Start).
        // Valid per EPUB3 and emitted by some toolchains.
        let opf = br##"<package>
            <metadata>
                <dc:title>A Game of Thrones</dc:title>
                <meta property="belongs-to-collection" id="c01">A Song of Ice and Fire</meta>
                <meta refines="#c01" property="group-position" content="3"/>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        let series = data.series_meta.unwrap();
        assert_eq!(series.name, "A Song of Ice and Fire");
        assert_eq!(series.position, Some(3.0));
    }

    #[test]
    fn epub3_collection_series_self_closing_reversed_order() {
        // Self-closing group-position BEFORE belongs-to-collection: order
        // independence must hold for the Event::Empty path too.
        let opf = br##"<package>
            <metadata>
                <dc:title>A Clash of Kings</dc:title>
                <meta refines="#c01" property="group-position" content="2"/>
                <meta property="belongs-to-collection" id="c01">A Song of Ice and Fire</meta>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        let series = data.series_meta.unwrap();
        assert_eq!(series.name, "A Song of Ice and Fire");
        assert_eq!(series.position, Some(2.0));
    }

    #[test]
    fn epub3_collection_series_self_closing_non_numeric_position() {
        // A malformed (non-numeric) content must not suppress the series name:
        // position degrades to None, series_meta itself stays Some.
        let opf = br##"<package>
            <metadata>
                <dc:title>A Storm of Swords</dc:title>
                <meta property="belongs-to-collection" id="c01">A Song of Ice and Fire</meta>
                <meta refines="#c01" property="group-position" content="three"/>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        let series = data.series_meta.unwrap();
        assert_eq!(series.name, "A Song of Ice and Fire");
        assert!(series.position.is_none());
    }

    #[test]
    fn epub3_collection_series_self_closing_fractional_position() {
        // EPUB3 allows fractional positions (0.5 prologues, 1.5 interludes);
        // the field is f64 end-to-end and must not truncate.
        let opf = br##"<package>
            <metadata>
                <dc:title>The Hedge Knight</dc:title>
                <meta property="belongs-to-collection" id="c01">A Song of Ice and Fire</meta>
                <meta refines="#c01" property="group-position" content="1.5"/>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        let series = data.series_meta.unwrap();
        assert_eq!(series.position, Some(1.5));
    }

    #[test]
    fn empty_metadata_returns_none_fields() {
        let opf = br"<package>
            <metadata/>
            <manifest/>
            <spine/>
        </package>";
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        assert!(data.title.is_none());
        assert!(data.creators.is_empty());
        assert!(data.description.is_none());
        assert!(data.identifiers.is_empty());
        assert!(data.series_meta.is_none());
    }

    #[test]
    fn multiple_creators_with_roles() {
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:creator opf:role="aut">Author One</dc:creator>
                <dc:creator opf:role="edt">Editor Two</dc:creator>
                <dc:creator>No Role Three</dc:creator>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        assert_eq!(data.creators.len(), 3);
        assert_eq!(data.creators[0].roles, vec!["aut"]);
        assert_eq!(data.creators[1].roles, vec!["edt"]);
        assert!(data.creators[2].roles.is_empty());
    }

    #[test]
    fn epub3_single_role_refine() {
        let opf = br##"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:creator id="c1">Ursula K. Le Guin</dc:creator>
                <meta refines="#c1" property="role" scheme="marc:relators">aut</meta>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.creators.len(), 1);
        assert_eq!(data.creators[0].name, "Ursula K. Le Guin");
        assert_eq!(data.creators[0].roles, vec!["aut"]);
        assert!(!data.creators[0].from_contributor);
    }

    #[test]
    fn epub3_multi_role_refine_on_one_creator() {
        // EPUB 3.3 §D.3.10 normative example: one creator may carry more than
        // one role refine (author AND illustrator here).
        let opf = br##"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:creator id="c1">Maurice Sendak</dc:creator>
                <meta refines="#c1" property="role" scheme="marc:relators">aut</meta>
                <meta refines="#c1" property="role" scheme="marc:relators">ill</meta>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.creators.len(), 1);
        assert_eq!(data.creators[0].roles, vec!["aut", "ill"]);
    }

    #[test]
    fn bare_dc_contributor_has_no_role() {
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:creator opf:role="aut">Primary Author</dc:creator>
                <dc:contributor>Some Helper</dc:contributor>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.creators.len(), 2);
        assert!(!data.creators[0].from_contributor);
        assert!(data.creators[1].from_contributor);
        assert!(data.creators[1].roles.is_empty());
    }

    #[test]
    fn declared_subtitle_extracted() {
        let opf = br##"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:title id="t1">Mistborn</dc:title>
                <dc:title id="t2">The Final Empire</dc:title>
                <meta refines="#t2" property="title-type">subtitle</meta>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.title.as_deref(), Some("Mistborn"));
        assert_eq!(data.subtitle.as_deref(), Some("The Final Empire"));
    }

    #[test]
    fn no_subtitle_refine_leaves_subtitle_none() {
        // No title-type refines at all: first title wins, subtitle stays
        // None (pre-EPUB3 behavior preserved).
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:title>Just A Title</dc:title>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.title.as_deref(), Some("Just A Title"));
        assert!(data.subtitle.is_none());
    }

    #[test]
    fn number_of_pages_extracted() {
        let opf = br#"<package>
            <metadata>
                <meta property="schema:numberOfPages">353</meta>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.number_of_pages.as_deref(), Some("353"));
    }

    #[test]
    fn cdata_title_extracts_character_data() {
        // CDATA is character data (XML 1.0 section 2.7); a title wrapped in
        // it must yield the same value as a plain text node, not the raw
        // `<![CDATA[...]]>` markup.
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:title><![CDATA[Main]]></dc:title>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.title.as_deref(), Some("Main"));
    }

    #[test]
    fn cdata_description_extracts_character_data() {
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:description><![CDATA[Blurb]]></dc:description>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.description.as_deref(), Some("Blurb"));
    }

    #[test]
    fn cdata_title_type_refine_resolves_subtitle() {
        // A title-type refine declared via CDATA must resolve exactly as its
        // text-node equivalent does; a raw-markup comparison would never
        // match "subtitle" and leave the field unset.
        let opf = br##"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:title id="t1">Mistborn</dc:title>
                <dc:title id="t2">The Final Empire</dc:title>
                <meta refines="#t2" property="title-type"><![CDATA[subtitle]]></meta>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.title.as_deref(), Some("Mistborn"));
        assert_eq!(data.subtitle.as_deref(), Some("The Final Empire"));
    }

    #[test]
    fn mixed_text_and_cdata_body_concatenates_in_document_order() {
        // A body mixing plain text and CDATA parts is legal XML; both are
        // character data, so the correct reading concatenates them in
        // document order rather than keeping only one part. The spaces
        // around the CDATA section belong to the body's interior and must
        // survive: they sit at the edges of the Text events the CDATA
        // section splits the body into, and `read_element_text` suspends
        // the reader's `trim_text` setting (needed to ignore insignificant
        // inter-element whitespace elsewhere) precisely so those edges are
        // not eaten.
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:description>Before <![CDATA[Middle]]> After</dc:description>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.description.as_deref(), Some("Before Middle After"));
    }

    #[test]
    fn text_node_entity_is_resolved() {
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:description>a &amp; b</dc:description>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.description.as_deref(), Some("a & b"));
    }

    #[test]
    fn nbsp_entity_resolves_in_element_text() {
        // `&nbsp;` is not an XML entity, but HTML-oriented EPUB tooling
        // emits it in metadata routinely and this pipeline has always
        // decoded it. Kept literal, it would reach the UI as markup and be
        // re-escaped to `&amp;nbsp;` in the source file by OPF writeback.
        // This pins the parse-layer boundary only: downstream whitespace
        // normalisation collapses the U+00A0 to a plain space, so this is
        // not an end-to-end NBSP preservation guarantee.
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:title>Foo&nbsp;Bar</dc:title>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.title.as_deref(), Some("Foo\u{00A0}Bar"));
    }

    #[test]
    fn cdata_body_entity_like_text_is_not_unescaped() {
        // CDATA content is never escaped: the five characters `&amp;` inside
        // a CDATA section are literal text, not an entity reference, unlike
        // the same characters in a text node.
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:description><![CDATA[&amp;]]></dc:description>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.description.as_deref(), Some("&amp;"));
    }

    #[test]
    fn unresolvable_char_ref_does_not_desync_parse() {
        // `&#xD800;` is a UTF-16 surrogate: syntactically a valid character
        // reference but not a legal XML character, so `resolve_char_ref`
        // errors. That error must not abort the element read mid-body: doing
        // so leaves the reader positioned just past the reference, still
        // inside `<dc:description>`, so the top-level loop misreads the rest
        // of the body as document-level content, including the nested
        // `<dc:title>` here, which would otherwise shadow the real title
        // that follows.
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:description>&#xD800;<dc:title>Fake</dc:title></dc:description>
                <dc:title>Real</dc:title>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(data.title.as_deref(), Some("Real"));
        assert!(
            data.description
                .as_deref()
                .is_some_and(|d| d.contains("&#xD800;"))
        );
    }

    #[test]
    fn truncated_document_missing_end_tag_yields_none() {
        // A document that ends before an open element's end tag must not
        // salvage the partial text seen so far: quick-xml delivers this as a
        // clean `Event::Eof` rather than an error (it does not validate tag
        // balance), so treating `Eof` as "found the end" would silently
        // return truncated content instead of signalling the missing field.
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:title>Cut off mid"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert!(data.title.is_none());
    }

    #[test]
    fn missing_everything_regression() {
        // No contributor/subtitle/pages metadata at all must still parse
        // cleanly, matching the pre-existing empty-metadata behavior.
        let opf = br"<package>
            <metadata/>
            <manifest/>
            <spine/>
        </package>";
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert!(data.title.is_none());
        assert!(data.subtitle.is_none());
        assert!(data.number_of_pages.is_none());
        assert!(data.creators.is_empty());
    }

    #[test]
    fn end_to_end_title_entity_decoded_exactly_once() {
        // XML 1.0 section 4.4.2: entity inclusion is one replacement
        // operation. A title meant to contain the literal text "A &amp; B"
        // must be encoded in the OPF as "A &amp;amp; B"; the parser resolves
        // the outer &amp; once, yielding "A &amp; B", and nothing downstream
        // (the metadata sanitiser included) may resolve it a second time.
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:title>A &amp;amp; B</dc:title>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        let extracted = crate::services::metadata::extractor::extract(&data);
        assert_eq!(extracted.title.as_deref(), Some("A &amp; B"));
    }

    #[test]
    fn end_to_end_numeric_reference_lookalike_decoded_exactly_once() {
        // Same shape as the amp case, but the literal text that must survive
        // looks like a numeric character reference ("&#65;"). The outer
        // &amp; is resolved once by the parser; the following "#65;" must
        // stay literal digits rather than be reinterpreted as a second
        // character reference by anything downstream.
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:title>Item &amp;#65; End</dc:title>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        let extracted = crate::services::metadata::extractor::extract(&data);
        assert_eq!(extracted.title.as_deref(), Some("Item &#65; End"));
    }

    #[test]
    fn end_to_end_cdata_entity_like_text_stays_literal() {
        // CDATA content is never escaped, so the five characters "&amp;"
        // inside a CDATA section are literal text. That must hold true
        // through the full validate -> extract pipeline, not just at the
        // parse layer.
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:description><![CDATA[&amp;]]></dc:description>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        let extracted = crate::services::metadata::extractor::extract(&data);
        assert_eq!(extracted.description.as_deref(), Some("&amp;"));
    }

    #[test]
    fn end_to_end_calibre_series_attribute_entity_decoded_exactly_once() {
        // calibre:series is attribute-sourced (Empty-form <meta>), not
        // element-text-sourced, so it exercises a different code path than
        // the dc:title/dc:description cases above: the attribute value must
        // be decoded at the parse boundary now that the sanitiser no longer
        // decodes, and still only once end to end.
        let opf = br#"<package>
            <metadata>
                <meta name="calibre:series" content="Tom &amp;amp; Jerry"/>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        let extracted = crate::services::metadata::extractor::extract(&data);
        assert_eq!(
            extracted.series.as_ref().map(|s| s.name.as_str()),
            Some("Tom &amp; Jerry")
        );
    }

    #[test]
    fn nbsp_entity_resolves_in_attribute_value() {
        // The strict attribute decoder is handed the same resolver as the
        // element-text path, so `&nbsp;` in a content attribute decodes on
        // the fast path without falling back to the lenient pass. Like the
        // element-text case above, this pins the parse-layer boundary, not
        // end-to-end NBSP survival.
        let opf = br#"<package>
            <metadata>
                <meta name="calibre:series" content="Foo&nbsp;Saga"/>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(
            data.series_meta.as_ref().map(|s| s.name.as_str()),
            Some("Foo\u{00A0}Saga")
        );
    }

    #[test]
    fn attribute_unknown_entity_degrades_per_reference() {
        // One unresolvable entity must not turn the whole attribute value
        // back into raw markup: strict decoding fails on `&hellip;`, and
        // the lenient pass still resolves the references around it,
        // keeping only the unknown one literal.
        let opf = br#"<package>
            <metadata>
                <meta name="calibre:series" content="Nightside &amp; Dawn &hellip; Saga &#233;"/>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(
            data.series_meta.as_ref().map(|s| s.name.as_str()),
            Some("Nightside & Dawn &hellip; Saga \u{00E9}")
        );
    }

    #[test]
    fn lenient_attribute_normalises_literal_whitespace_before_references() {
        // The lenient pass must normalise literal whitespace before
        // expanding references, matching strict attribute-value
        // normalisation ordering: the literal tab in the value becomes a
        // space, while the tab produced by `&#x9;` survives as itself. The
        // unknown entity forces the strict decoder to fail so the whole
        // value goes through the lenient path.
        let tab = '\t';
        let opf = format!(
            r#"<package>
            <metadata>
                <meta name="calibre:series" content="A&#x9;B{tab}C &bogus;"/>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#
        );
        let handle = make_handle(opf.as_bytes());
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(
            data.series_meta.as_ref().map(|s| s.name.as_str()),
            Some("A\tB C &bogus;")
        );
    }

    #[test]
    fn attribute_bare_ampersand_keeps_the_rest_decoded() {
        // A bare `&` is not well-formed XML but appears in real-world OPF
        // attribute values. It fails strict decoding outright, so the
        // lenient pass must keep it literal while the well-formed
        // references elsewhere in the value still decode.
        let opf = br#"<package>
            <metadata>
                <meta name="calibre:series" content="Tom & Jerry &amp; Co"/>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let data = validate(&handle, Some("OEBPS/content.opf"), &mut issues).unwrap();
        assert_eq!(
            data.series_meta.as_ref().map(|s| s.name.as_str()),
            Some("Tom & Jerry & Co")
        );
    }
}

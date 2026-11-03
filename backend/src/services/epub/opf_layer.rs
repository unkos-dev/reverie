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
use quick_xml::XmlVersion;
use quick_xml::escape::resolve_xml_entity;
use quick_xml::events::Event;
use quick_xml::events::attributes::Attribute;
use quick_xml::name::QName;
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

/// A supplied identifier with its bibliographic declaration and package role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identifier {
    /// Identifier text before ISBN parsing.
    pub value: String,
    /// Element identifier for EPUB refinements.
    pub id: Option<String>,
    /// Whether the package selects this identifier as its identity.
    pub package_identity: bool,
    /// Whether a scheme or identifier-type refinement declares an ISBN.
    pub declared_isbn: bool,
}

impl From<&str> for Identifier {
    fn from(value: &str) -> Self {
        Self {
            value: value.into(),
            id: None,
            package_identity: false,
            declared_isbn: false,
        }
    }
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
    pub identifiers: Vec<Identifier>,
    /// Dublin Core: subject values
    pub subjects: Vec<String>,
    /// Series metadata (calibre or `EPUB3` collection)
    pub series_meta: Option<SeriesMeta>,
}

/// Extract the local name from a possibly-namespaced element name.
/// e.g. `dc:title` → `title`, `title` → `title`
fn local_name(name: &str) -> &str {
    name.split_once(':').map_or(name, |(_, local)| local)
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
fn lenient_attr_text(a: &Attribute<'_>) -> String {
    let raw = a.value.as_ref();

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
    result
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
fn attr_text(a: &Attribute<'_>) -> String {
    a.normalized_value_with(XmlVersion::Implicit1_0, 1, resolve_metadata_entity)
        .map_or_else(|_| lenient_attr_text(a), std::borrow::Cow::into_owned)
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
/// `Text` is read as valid UTF-8 and its split-out `GeneralRef` entities resolved,
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
            Event::Text(t) => text.push_str(t.as_ref()),
            Event::CData(c) => text.push_str(c.as_ref()),
            Event::GeneralRef(r) => match r.resolve_char_ref() {
                Ok(Some(ch)) => text.push(ch),
                Ok(None) => {
                    let name = r.as_ref();
                    if let Some(resolved) = resolve_metadata_entity(name) {
                        text.push_str(resolved);
                    } else {
                        // Not a resolvable named entity (the predefined
                        // set plus nbsp) and not a character reference;
                        // without a DTD there is nothing to resolve it
                        // against, so keep the original markup rather
                        // than lose it.
                        text.push('&');
                        text.push_str(name);
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
                    let name = r.as_ref();
                    text.push('&');
                    text.push_str(name);
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

/// Whether the document parses to its end with every element closed.
fn is_balanced(xml: &str) -> bool {
    let mut reader = Reader::from_str(xml);
    let mut depth: usize = 0;
    loop {
        match reader.read_event() {
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(_)) => depth = depth.saturating_sub(1),
            Ok(Event::Eof) => return depth == 0,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
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
/// `handle`, the entry bytes are not valid `UTF-8`, or the document is not
/// well-formed to its end (a parse error or an unclosed element).
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
    if !is_balanced(xml) {
        return None;
    }

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
    let mut labelled_date: Option<String> = None;
    let mut epub3 = false;
    let mut language: Option<String> = None;
    let mut identifiers: Vec<Identifier> = Vec::new();
    let mut package_identifier = None;
    let mut isbn_refines = HashSet::new();
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
            Event::Start(e) if e.name().as_ref() == "meta" => {
                let e = e.into_owned(); // release reader buffer borrow before read_element_text
                let prop = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == "property")
                    .map(|a| a.value.into_owned());
                let content_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == "content")
                    .map(|a| attr_text(&a));
                let id_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == "id")
                    .map(|a| a.value.into_owned());
                let refines_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == "refines")
                    .map(|a| a.value.into_owned());
                let name_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == "name")
                    .map(|a| a.value.into_owned());

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
                    if prop == "identifier-type" {
                        let value =
                            content_attr.or_else(|| read_element_text(&mut reader, e.name()));
                        if let Some(refines) = refines_attr
                            && value.as_deref().is_some_and(|v| {
                                v.trim() == "15" || v.trim().eq_ignore_ascii_case("isbn")
                            })
                        {
                            isbn_refines.insert(refines);
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
            Event::Empty(e) if e.name().as_ref() == "meta" => {
                let prop = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == "property")
                    .map(|a| a.value.into_owned());
                let name_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == "name")
                    .map(|a| a.value.into_owned());
                let content = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == "content")
                    .map(|a| attr_text(&a));
                let refines_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == "refines")
                    .map(|a| a.value.into_owned());

                if prop.as_deref() == Some("identifier-type") {
                    if let Some(refines) = refines_attr.clone()
                        && content.as_deref().is_some_and(|v| {
                            v.trim() == "15" || v.trim().eq_ignore_ascii_case("isbn")
                        })
                    {
                        isbn_refines.insert(refines);
                    }
                    continue;
                }

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
                    "title"
                        | "creator"
                        | "contributor"
                        | "description"
                        | "publisher"
                        | "date"
                        | "language"
                        | "identifier"
                        | "subject"
                ) && e.name().as_ref() != "meta" =>
            {
                let local = local_name(e.name().as_ref()).to_owned();
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
                        k == "opf:role" || k == "role"
                    })
                    .map(|a| a.value.into_owned());
                let id_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == "id")
                    .map(|a| a.value.into_owned());

                let declared_isbn = e.attributes().flatten().any(|a| {
                    local_name(a.key.as_ref()) == "scheme"
                        && attr_text(&a).eq_ignore_ascii_case("isbn")
                });
                let publication_event = e
                    .attributes()
                    .flatten()
                    .find(|attribute| local_name(attribute.key.as_ref()) == "event")
                    .map(|attribute| attribute.value.into_owned());
                let e = e.into_owned();
                let text = read_element_text(&mut reader, e.name())
                    .map(|t| t.trim().to_string())
                    .filter(|s| !s.is_empty());

                if let Some(text) = text {
                    match local.as_str() {
                        "title" => title_elements.push((id_attr, text)),
                        "creator" => creator_bufs.push(CreatorBuf {
                            id: id_attr,
                            name: text,
                            inline_role,
                            from_contributor: false,
                        }),
                        "contributor" => creator_bufs.push(CreatorBuf {
                            id: id_attr,
                            name: text,
                            inline_role,
                            from_contributor: true,
                        }),
                        "description" if description.is_none() => description = Some(text),
                        "publisher" if publisher.is_none() => publisher = Some(text),
                        "date" => match publication_event.as_deref() {
                            Some("publication") if !epub3 && labelled_date.is_none() => {
                                labelled_date = Some(text);
                            }
                            None | Some("publication") if date.is_none() => date = Some(text),
                            _ => {}
                        },
                        "language" if language.is_none() => language = Some(text),
                        "identifier" => identifiers.push(Identifier {
                            value: text,
                            id: id_attr,
                            package_identity: false,
                            declared_isbn,
                        }),
                        "subject" => subjects.push(text),
                        _ => {}
                    }
                }
            }
            // General arm — meta and DC already handled by guarded arms above
            Event::Empty(e) | Event::Start(e) => match e.name().as_ref() {
                name if local_name(name) == "package" => {
                    package_identifier = e
                        .attributes()
                        .flatten()
                        .find(|a| a.key.as_ref() == "unique-identifier")
                        .map(|a| attr_text(&a));
                    epub3 = e.attributes().flatten().any(|attribute| {
                        attribute.key.as_ref() == "version" && attribute.value.starts_with('3')
                    });
                }
                "item" => {
                    let attrs: HashMap<String, String> = e
                        .attributes()
                        .flatten()
                        .map(|a| (a.key.as_ref().to_owned(), a.value.into_owned()))
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
                "itemref" => {
                    if let Some(idref) =
                        e.attributes().flatten().find(|a| a.key.as_ref() == "idref")
                    {
                        spine_idrefs.push(idref.value.into_owned());
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

    for identifier in &mut identifiers {
        identifier.package_identity =
            identifier.id.is_some() && identifier.id == package_identifier;
        if let Some(id) = &identifier.id {
            identifier.declared_isbn |= isbn_refines.contains(&format!("#{id}"));
        }
    }

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
        date: labelled_date.or(date),
        language,
        identifiers,
        subjects,
        series_meta,
    })
}

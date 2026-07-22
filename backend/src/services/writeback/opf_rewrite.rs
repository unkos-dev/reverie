//! Pure-function OPF XML rewriter.
//!
//! Streams the source OPF as `quick_xml` events, replaces targeted Dublin
//! Core / `<meta>` elements in place, and preserves everything else
//! byte-for-byte (unknown namespaces, custom `<meta>`, `<dc:coverage>`,
//! prologue, declaration order).  Callers pass a `Target` carrying the
//! desired per-field values; `None` for a field means "leave the OPF's
//! current value alone".
//!
//! Memory-instinct: never use string substitution or regex on XML.  All
//! transforms go through `quick-xml` events.

use quick_xml::Reader;
use quick_xml::Writer;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use std::collections::HashMap;
use std::io::Cursor;

use super::error::WritebackError;

/// Target values to write back into the `OPF`.  Each field semantics:
///
/// - `None`: leave the `OPF`'s current value alone.
/// - `Some(v)`: set the Dublin Core element's text content to `v`.  An empty
///   `v` is written as an empty element (not deleted) — the MVP does not
///   delete existing `OPF` fields.
#[derive(Debug, Default, Clone)]
pub struct Target<'a> {
    /// `dc:title` text.  Applied to the main title element only: a `dc:title`
    /// carrying a `title-type` refine other than `main` is never rewritten
    /// from this field.
    pub title: Option<&'a str>,
    /// Text for a `dc:title` refined `title-type="subtitle"`.  `None` leaves a
    /// declared subtitle untouched.  A subtitle is only ever written into an
    /// element the `OPF` already declares; the rewriter does not synthesize a
    /// subtitle element (and its refine) for a file that has none.
    pub subtitle: Option<&'a str>,
    /// `dc:description` text (blurb / synopsis).
    pub description: Option<&'a str>,
    /// `dc:language` BCP 47 tag (e.g. `"en"`, `"fr"`).
    pub language: Option<&'a str>,
    /// `dc:publisher` text.
    pub publisher: Option<&'a str>,
    /// ISO 8601 date string (`YYYY-MM-DD` or full timestamp).
    pub pub_date: Option<&'a str>,
    /// `ISBN`-10 string; used as fallback when `isbn_13` is absent.
    pub isbn_10: Option<&'a str>,
    /// `ISBN`-13 string; preferred over `isbn_10` when both are set.
    pub isbn_13: Option<&'a str>,
    /// Series name and optional position; written as `belongs-to-collection`
    /// (`EPUB3`) and/or `calibre:series` (`EPUB2` or when both forms exist).
    pub series: Option<SeriesRef<'a>>,
}

/// Series membership reference used by `Target`.
#[derive(Debug, Clone, Copy)]
pub struct SeriesRef<'a> {
    /// Display name of the series (e.g. `"Mistborn"`).
    pub name: &'a str,
    /// One-based reading order position within the series; `None` when unknown.
    pub index: Option<f64>,
}

/// Apply `target` to the `OPF` bytes at `opf_bytes`, returning the rewritten
/// `UTF-8` `XML`.  Preserves all non-targeted elements, attributes, and
/// whitespace.
///
/// Performs two passes: a scan pass to detect `EPUB` version, existing
/// `calibre:series`, and `belongs-to-collection` markers; then a rewrite
/// pass that replaces targeted Dublin Core elements in place and appends
/// missing identifiers/series metadata before the closing `</metadata>` tag.
///
/// # Errors
///
/// - `WritebackError::Xml` — `quick_xml` failed to parse the input bytes or
///   write an event to the output buffer.
#[expect(
    clippy::too_many_lines,
    reason = "transform implements a two-pass XML rewrite over 11 metadata axes plus EPUB 2/3 version-specific formats; the passes share state that cannot be cleanly separated without significant architecture changes"
)]
pub fn transform(opf_bytes: &[u8], target: &Target<'_>) -> Result<Vec<u8>, WritebackError> {
    // Own every event up front so we can walk the stream twice — once to
    // detect EPUB version / presence of ISBN identifier / series markers,
    // once to rewrite.
    let events = read_all_events(opf_bytes)?;

    let meta = scan_metadata(&events);

    // Rewrite pass.
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut in_metadata_depth: Option<usize> = None;
    let mut depth: usize = 0;
    let mut belongs_to_written = false;
    let mut calibre_series_written = false;
    let mut isbn_identifier_written = false;

    // If the file uses EPUB 3, we write belongs-to-collection.  If it ALSO
    // has pre-existing calibre:series meta, we update both (downstream-reader
    // compatibility).  If the file is EPUB 2, we write only calibre:series.
    let use_belongs_to = meta.epub_version.starts_with('3');
    let use_calibre = !meta.epub_version.starts_with('3') || meta.had_calibre_series;
    let use_belongs = use_belongs_to || meta.had_belongs_to_collection;

    let mut i = 0;
    while i < events.len() {
        let ev = &events[i];
        match ev {
            Event::Start(e) => {
                let name_bytes = e.name().as_ref().to_vec();
                depth += 1;
                let local = local_name(&name_bytes).to_vec();

                if local == b"metadata" && in_metadata_depth.is_none() {
                    in_metadata_depth = Some(depth);
                    writer.write_event(Event::Start(e.clone()))?;
                    i += 1;
                    continue;
                }

                if in_metadata_depth.is_some() {
                    // Targeted DC elements: replace text.
                    if let Some(new_text) = target_text_for_dc(&local, e, target, &meta.title_types)
                    {
                        // Skip everything until the matching End tag.
                        write_replaced_element(&mut writer, e, new_text)?;
                        i = skip_to_matching_end(&events, i) + 1;
                        depth -= 1;
                        continue;
                    }
                    // dc:identifier — update if opf:scheme matches ISBN.
                    if local == b"identifier"
                        && is_isbn_identifier(e)
                        && let Some(new_isbn) = target.isbn_13.or(target.isbn_10)
                    {
                        write_replaced_element(&mut writer, e, new_isbn)?;
                        isbn_identifier_written = true;
                        i = skip_to_matching_end(&events, i) + 1;
                        depth -= 1;
                        continue;
                    }
                    // <meta property="belongs-to-collection"> — update text + refinement metas.
                    if local == b"meta"
                        && has_attr(e, b"property", b"belongs-to-collection")
                        && let Some(series) = target.series
                        && use_belongs
                    {
                        let id = attr_value(e, b"id").unwrap_or_else(|| b"series-1".to_vec());
                        write_belongs_to_collection(&mut writer, &id, series.name)?;
                        i = skip_to_matching_end(&events, i) + 1;
                        depth -= 1;
                        belongs_to_written = true;
                        // Best-effort: don't drop refinement metas that follow.
                        // The downstream writer rewrites them if they match id.
                        // For MVP we also emit fresh refinements at the end.
                        continue;
                    }
                }

                writer.write_event(Event::Start(e.clone()))?;
            }
            Event::Empty(e) => {
                let name_bytes = e.name().as_ref().to_vec();
                let local = local_name(&name_bytes).to_vec();

                if in_metadata_depth.is_some() {
                    if let Some(new_text) = target_text_for_dc(&local, e, target, &meta.title_types)
                    {
                        // Rare: <dc:title/> as empty element — expand to paired.
                        write_replaced_element_from_empty(&mut writer, e, new_text)?;
                        i += 1;
                        continue;
                    }
                    if local == b"identifier"
                        && is_isbn_identifier(e)
                        && let Some(new_isbn) = target.isbn_13.or(target.isbn_10)
                    {
                        write_replaced_element_from_empty(&mut writer, e, new_isbn)?;
                        isbn_identifier_written = true;
                        i += 1;
                        continue;
                    }
                    // <meta name="calibre:series" content="..."/>
                    if local == b"meta"
                        && has_attr(e, b"name", b"calibre:series")
                        && let Some(series) = target.series
                        && use_calibre
                    {
                        write_calibre_series(&mut writer, series.name)?;
                        i += 1;
                        calibre_series_written = true;
                        continue;
                    }
                    if local == b"meta"
                        && has_attr(e, b"name", b"calibre:series_index")
                        && let Some(series) = target.series
                        && use_calibre
                    {
                        write_calibre_series_index(&mut writer, series.index)?;
                        i += 1;
                        continue;
                    }
                }

                writer.write_event(Event::Empty(e.clone()))?;
            }
            Event::End(e) => {
                let name_bytes = e.name().as_ref().to_vec();
                depth -= 1;
                let local = local_name(&name_bytes).to_vec();

                // Before closing <metadata>, insert any fresh elements we need.
                if in_metadata_depth == Some(depth + 1) && local == b"metadata" {
                    // ISBN insertion when absent.
                    if !isbn_identifier_written
                        && let Some(new_isbn) = target.isbn_13.or(target.isbn_10)
                        && !new_isbn.is_empty()
                    {
                        write_new_isbn_identifier(&mut writer, new_isbn)?;
                    }
                    // belongs-to-collection insertion when absent.
                    if !belongs_to_written
                        && use_belongs_to
                        && let Some(series) = target.series
                    {
                        let id = b"series-1";
                        write_belongs_to_collection(&mut writer, id, series.name)?;
                        write_belongs_to_collection_refinements(&mut writer, id, series.index)?;
                    }
                    // calibre:series insertion when absent.
                    if !calibre_series_written
                        && use_calibre
                        && let Some(series) = target.series
                    {
                        write_calibre_series(&mut writer, series.name)?;
                        write_calibre_series_index(&mut writer, series.index)?;
                    }
                    in_metadata_depth = None;
                }

                writer.write_event(Event::End(e.clone()))?;
            }
            other => {
                writer.write_event(other.clone())?;
            }
        }
        i += 1;
    }

    Ok(writer.into_inner().into_inner())
}

// ── Scan pass metadata ────────────────────────────────────────────────────

struct ScanResult {
    epub_version: String,
    had_belongs_to_collection: bool,
    had_calibre_series: bool,
    /// Declared `title-type` per refined element `id` (the `refines`
    /// attribute with its leading `#` stripped), lowercased.
    title_types: HashMap<Vec<u8>, String>,
}

fn scan_metadata(events: &[Event<'static>]) -> ScanResult {
    let mut epub_version = "3.0".to_string();
    let mut had_belongs = false;
    let mut had_calibre = false;
    let mut title_types: HashMap<Vec<u8>, String> = HashMap::new();
    for (i, ev) in events.iter().enumerate() {
        match ev {
            Event::Start(e) => {
                let local = local_name(e.name().as_ref()).to_vec();
                if local == b"package"
                    && let Some(v) = attr_value_as_string(e, b"version")
                {
                    epub_version = v;
                }
                if local == b"meta" && has_attr(e, b"property", b"belongs-to-collection") {
                    had_belongs = true;
                }
                // `<meta refines="#t2" property="title-type">subtitle</meta>`:
                // the value is the element's text body.
                if local == b"meta"
                    && has_attr(e, b"property", b"title-type")
                    && let Some(refines) = attr_value(e, b"refines")
                    && let Some(value) =
                        attr_value_as_string(e, b"content").or_else(|| element_text(events, i))
                {
                    title_types.insert(strip_fragment(&refines), normalize_title_type(&value));
                }
            }
            Event::Empty(e) => {
                let local = local_name(e.name().as_ref()).to_vec();
                if local == b"meta" && has_attr(e, b"name", b"calibre:series") {
                    had_calibre = true;
                }
                // Self-closing refine: the value lives in `content`, and
                // quick-xml never fires a Text event for it.
                if local == b"meta"
                    && has_attr(e, b"property", b"title-type")
                    && let Some(refines) = attr_value(e, b"refines")
                    && let Some(value) = attr_value_as_string(e, b"content")
                {
                    title_types.insert(strip_fragment(&refines), normalize_title_type(&value));
                }
            }
            _ => {}
        }
    }
    ScanResult {
        epub_version,
        had_belongs_to_collection: had_belongs,
        had_calibre_series: had_calibre,
        title_types,
    }
}

/// Text body of the element starting at `start_idx`, empty text discarded.
fn element_text(events: &[Event<'static>], start_idx: usize) -> Option<String> {
    let end = skip_to_matching_end(events, start_idx);
    events[start_idx + 1..end.min(events.len())]
        .iter()
        .find_map(|ev| match ev {
            Event::Text(t) => t.decode().ok().map(|s| s.trim().to_string()),
            _ => None,
        })
        .filter(|s| !s.is_empty())
}

/// `refines="#t2"` addresses the element whose `id` is `t2`.
fn strip_fragment(refines: &[u8]) -> Vec<u8> {
    refines.strip_prefix(b"#").unwrap_or(refines).to_vec()
}

fn normalize_title_type(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

// ── Event-writing helpers ─────────────────────────────────────────────────

fn write_replaced_element(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    start: &BytesStart<'_>,
    new_text: &str,
) -> Result<(), WritebackError> {
    let name_bytes = start.name().as_ref().to_vec();
    writer.write_event(Event::Start(start.clone()))?;
    writer.write_event(Event::Text(BytesText::new(new_text)))?;
    writer.write_event(Event::End(BytesEnd::new(
        String::from_utf8_lossy(&name_bytes).to_string(),
    )))?;
    Ok(())
}

fn write_replaced_element_from_empty(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    empty: &BytesStart<'_>,
    new_text: &str,
) -> Result<(), WritebackError> {
    let name_bytes = empty.name().as_ref().to_vec();
    let mut start = BytesStart::new(String::from_utf8_lossy(&name_bytes).to_string());
    for attr in empty.attributes().flatten() {
        start.push_attribute(attr);
    }
    writer.write_event(Event::Start(start))?;
    writer.write_event(Event::Text(BytesText::new(new_text)))?;
    writer.write_event(Event::End(BytesEnd::new(
        String::from_utf8_lossy(&name_bytes).to_string(),
    )))?;
    Ok(())
}

fn write_new_isbn_identifier(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    isbn: &str,
) -> Result<(), WritebackError> {
    let mut start = BytesStart::new("dc:identifier");
    start.push_attribute(("opf:scheme", "ISBN"));
    writer.write_event(Event::Start(start))?;
    writer.write_event(Event::Text(BytesText::new(isbn)))?;
    writer.write_event(Event::End(BytesEnd::new("dc:identifier")))?;
    Ok(())
}

fn write_belongs_to_collection(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    id: &[u8],
    name: &str,
) -> Result<(), WritebackError> {
    let id_str = String::from_utf8_lossy(id);
    let mut start = BytesStart::new("meta");
    start.push_attribute(("property", "belongs-to-collection"));
    start.push_attribute(("id", id_str.as_ref()));
    writer.write_event(Event::Start(start))?;
    writer.write_event(Event::Text(BytesText::new(name)))?;
    writer.write_event(Event::End(BytesEnd::new("meta")))?;
    Ok(())
}

fn write_belongs_to_collection_refinements(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    id: &[u8],
    index: Option<f64>,
) -> Result<(), WritebackError> {
    let refines_target = format!("#{}", String::from_utf8_lossy(id));
    let mut t = BytesStart::new("meta");
    t.push_attribute(("refines", refines_target.as_str()));
    t.push_attribute(("property", "collection-type"));
    writer.write_event(Event::Start(t))?;
    writer.write_event(Event::Text(BytesText::new("series")))?;
    writer.write_event(Event::End(BytesEnd::new("meta")))?;

    if let Some(idx) = index {
        let mut t = BytesStart::new("meta");
        t.push_attribute(("refines", refines_target.as_str()));
        t.push_attribute(("property", "group-position"));
        writer.write_event(Event::Start(t))?;
        let idx_str = format_index(idx);
        writer.write_event(Event::Text(BytesText::new(&idx_str)))?;
        writer.write_event(Event::End(BytesEnd::new("meta")))?;
    }
    Ok(())
}

fn write_calibre_series(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
) -> Result<(), WritebackError> {
    let mut el = BytesStart::new("meta");
    el.push_attribute(("name", "calibre:series"));
    el.push_attribute(("content", name));
    writer.write_event(Event::Empty(el))?;
    Ok(())
}

fn write_calibre_series_index(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    index: Option<f64>,
) -> Result<(), WritebackError> {
    if let Some(idx) = index {
        let mut el = BytesStart::new("meta");
        let idx_str = format_index(idx);
        el.push_attribute(("name", "calibre:series_index"));
        el.push_attribute(("content", idx_str.as_str()));
        writer.write_event(Event::Empty(el))?;
    }
    Ok(())
}

// ── Lookup helpers ────────────────────────────────────────────────────────

/// Resolve the replacement text for a Dublin Core element, or `None` to leave
/// it byte-for-byte alone.
///
/// `dc:title` is resolved against the element's `title-type` refine: an
/// unrefined (or `main`-refined) title takes `target.title`, a `subtitle`
/// refine takes `target.subtitle`, and any other declared type (`short`,
/// `collection`, `edition`, `extended`) is left alone because the canonical
/// layer does not model it and writing the main title over it would destroy
/// the declared value.
fn target_text_for_dc<'t>(
    local: &[u8],
    el: &BytesStart<'_>,
    target: &'t Target<'_>,
    title_types: &HashMap<Vec<u8>, String>,
) -> Option<&'t str> {
    match local {
        b"title" => match title_type_of(el, title_types) {
            Some("subtitle") => target.subtitle,
            Some(other) if other != "main" => None,
            _ => target.title,
        },
        b"description" => target.description,
        b"language" => target.language,
        b"publisher" => target.publisher,
        b"date" => target.pub_date,
        _ => None,
    }
}

/// The `title-type` refine declared for `el`, looked up by the element's `id`.
fn title_type_of<'m>(
    el: &BytesStart<'_>,
    title_types: &'m HashMap<Vec<u8>, String>,
) -> Option<&'m str> {
    let id = attr_value(el, b"id")?;
    title_types.get(&id).map(String::as_str)
}

fn has_attr(start: &BytesStart<'_>, name: &[u8], value: &[u8]) -> bool {
    for attr in start.attributes().flatten() {
        if local_name(attr.key.as_ref()) == name && attr.value.as_ref().eq_ignore_ascii_case(value)
        {
            return true;
        }
    }
    false
}

fn attr_value(start: &BytesStart<'_>, name: &[u8]) -> Option<Vec<u8>> {
    for attr in start.attributes().flatten() {
        if local_name(attr.key.as_ref()) == name {
            return Some(attr.value.into_owned());
        }
    }
    None
}

fn attr_value_as_string(start: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    attr_value(start, name).map(|v| String::from_utf8_lossy(&v).into_owned())
}

fn is_isbn_identifier(start: &BytesStart<'_>) -> bool {
    for attr in start.attributes().flatten() {
        if local_name(attr.key.as_ref()) == b"scheme"
            && attr.value.as_ref().eq_ignore_ascii_case(b"ISBN")
        {
            return true;
        }
    }
    false
}

fn local_name(name: &[u8]) -> &[u8] {
    name.iter()
        .position(|&b| b == b':')
        .map_or(name, |pos| &name[pos + 1..])
}

fn format_index(idx: f64) -> String {
    if idx.fract() == 0.0 {
        // idx.fract() == 0.0 guarantees the value is a whole number;
        // series positions are always small integers well within i64 range.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "fract()==0 guarantees a whole number; series positions are small integers well within i64"
        )]
        let whole: i64 = idx as i64;
        format!("{whole}")
    } else {
        format!("{idx}")
    }
}

fn skip_to_matching_end(events: &[Event<'static>], start_idx: usize) -> usize {
    // Walk forward until we find the matching End tag, accounting for
    // nested Start/End pairs.
    let start_name = match &events[start_idx] {
        Event::Start(e) => e.name().as_ref().to_vec(),
        _ => return start_idx,
    };
    let mut depth = 1usize;
    let mut i = start_idx + 1;
    while i < events.len() {
        match &events[i] {
            Event::Start(e) if e.name().as_ref() == start_name.as_slice() => {
                depth += 1;
            }
            Event::End(e) if e.name().as_ref() == start_name.as_slice() => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    i
}

fn read_all_events(opf_bytes: &[u8]) -> Result<Vec<Event<'static>>, WritebackError> {
    let mut reader = Reader::from_reader(opf_bytes);
    reader.config_mut().trim_text(false);
    let mut events: Vec<Event<'static>> = Vec::new();
    let mut buf = Vec::new();
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(WritebackError::Xml)?
        {
            Event::Eof => return Ok(events),
            other => events.push(other.into_owned()),
        }
        buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_epub3(metadata_extra: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="pub-id" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
  <metadata>
    <dc:identifier id="pub-id">urn:uuid:A1B2C3D4</dc:identifier>
    <dc:title>Old Title</dc:title>
    <dc:language>en</dc:language>
    <dc:creator>Old Author</dc:creator>
    {metadata_extra}
  </metadata>
  <manifest/>
  <spine/>
</package>"#
        )
    }

    fn sample_epub2(metadata_extra: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="2.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
  <metadata>
    <dc:identifier id="bookid" opf:scheme="UUID">urn:uuid:1234</dc:identifier>
    <dc:title>Old Title</dc:title>
    <dc:language>en</dc:language>
    {metadata_extra}
  </metadata>
  <manifest/>
  <spine/>
</package>"#
        )
    }

    fn transform_str(input: &str, target: &Target<'_>) -> String {
        let out = transform(input.as_bytes(), target).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn transform_replaces_dc_title() {
        let input = sample_epub3("");
        let target = Target {
            title: Some("New Title"),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(out.contains("<dc:title>New Title</dc:title>"), "got: {out}");
        assert!(!out.contains("Old Title"), "old title leaked: {out}");
    }

    /// EPUB3 main title + declared subtitle, both carrying `title-type`
    /// refines, in the order Standard Ebooks and Calibre emit them.
    fn sample_epub3_with_subtitle() -> String {
        sample_epub3_titles(
            r##"<dc:title id="t1">Old Title</dc:title>
    <dc:title id="t2">Old Subtitle</dc:title>
    <meta refines="#t1" property="title-type">main</meta>
    <meta refines="#t2" property="title-type">subtitle</meta>"##,
        )
    }

    /// `sample_epub3` with its single unrefined `dc:title` replaced by
    /// `titles`, so title-shaped cases control the whole title block.
    fn sample_epub3_titles(titles: &str) -> String {
        sample_epub3("").replace("<dc:title>Old Title</dc:title>", titles)
    }

    #[test]
    fn transform_rewrites_subtitle_from_canonical_subtitle() {
        let input = sample_epub3_with_subtitle();
        let target = Target {
            title: Some("New Title"),
            subtitle: Some("New Subtitle"),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(
            out.contains(r#"<dc:title id="t1">New Title</dc:title>"#),
            "main title not rewritten: {out}"
        );
        assert!(
            out.contains(r#"<dc:title id="t2">New Subtitle</dc:title>"#),
            "subtitle not rewritten from the canonical subtitle: {out}"
        );
        assert!(!out.contains("Old Title"), "old title leaked: {out}");
        assert!(!out.contains("Old Subtitle"), "old subtitle leaked: {out}");
    }

    #[test]
    fn transform_preserves_subtitle_when_canonical_subtitle_absent() {
        // The regression this file exists for: a title-only writeback used to
        // stomp the declared subtitle with the main title.
        let input = sample_epub3_with_subtitle();
        let target = Target {
            title: Some("New Title"),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(
            out.contains(r#"<dc:title id="t2">Old Subtitle</dc:title>"#),
            "declared subtitle was destroyed: {out}"
        );
        assert!(
            out.contains(r#"<dc:title id="t1">New Title</dc:title>"#),
            "main title not rewritten: {out}"
        );
    }

    #[test]
    fn transform_round_trips_subtitle_through_extraction() {
        // extract → writeback → re-extract must leave the subtitle intact.
        // A second writeback pass models the repeated PATCH case, where the
        // stomp used to poison the canonical layer with subtitle == title.
        let input = sample_epub3_with_subtitle();
        let target = Target {
            title: Some("New Title"),
            subtitle: Some("Old Subtitle"),
            ..Default::default()
        };
        let once = transform_str(&input, &target);
        let twice = transform_str(&once, &target);
        let data = extract_opf(twice.as_bytes());
        assert_eq!(data.title.as_deref(), Some("New Title"));
        assert_eq!(data.subtitle.as_deref(), Some("Old Subtitle"));
    }

    /// Run the real ingestion-side OPF parser over rewritten bytes, so the
    /// round-trip assertion is against extraction rather than a restatement
    /// of the rewriter's own rules.
    fn extract_opf(opf_bytes: &[u8]) -> crate::services::epub::opf_layer::OpfData {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
            zip::write::FileOptions::default();
        w.start_file("OEBPS/content.opf", opts).unwrap();
        w.write_all(opf_bytes).unwrap();
        let handle = crate::services::epub::zip_layer::ZipHandle {
            bytes: w.finish().unwrap().into_inner(),
            entries: vec!["OEBPS/content.opf".to_string()],
        };
        let mut issues = Vec::new();
        crate::services::epub::opf_layer::validate(&handle, Some("OEBPS/content.opf"), &mut issues)
            .unwrap()
    }

    #[test]
    fn transform_preserves_self_closing_subtitle_refine() {
        // `<meta refines=".." property="title-type" content="subtitle"/>`
        // carries its value in an attribute; quick-xml fires Empty, not Start.
        let input = sample_epub3_titles(
            r##"<dc:title id="t1">Old Title</dc:title>
    <dc:title id="t2">Old Subtitle</dc:title>
    <meta refines="#t2" property="title-type" content="subtitle"/>"##,
        );
        let target = Target {
            title: Some("New Title"),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(
            out.contains(r#"<dc:title id="t2">Old Subtitle</dc:title>"#),
            "self-closing subtitle refine was not honored: {out}"
        );
    }

    #[test]
    fn transform_leaves_non_main_title_types_alone() {
        // `short` is not modelled canonically; rewriting it from the main
        // title would destroy the declared value the same way the subtitle
        // stomp did.
        let input = sample_epub3_titles(
            r##"<dc:title id="t1">Old Title</dc:title>
    <dc:title id="t3">Old Short</dc:title>
    <meta refines="#t3" property="title-type">short</meta>"##,
        );
        let target = Target {
            title: Some("New Title"),
            subtitle: Some("New Subtitle"),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(
            out.contains(r#"<dc:title id="t3">Old Short</dc:title>"#),
            "short title was rewritten: {out}"
        );
        assert!(
            out.contains(r#"<dc:title id="t1">New Title</dc:title>"#),
            "main title not rewritten: {out}"
        );
    }

    #[test]
    fn transform_does_not_synthesize_a_subtitle_element() {
        // No declared subtitle means no element to write into; the rewriter
        // must not invent one (or a refine addressing a nonexistent id).
        let input = sample_epub3("");
        let target = Target {
            title: Some("New Title"),
            subtitle: Some("New Subtitle"),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(
            !out.contains("New Subtitle") && !out.contains("title-type"),
            "subtitle element synthesized: {out}"
        );
    }

    #[test]
    fn transform_replaces_dc_description() {
        let input = sample_epub3(r"<dc:description>Old blurb</dc:description>");
        let target = Target {
            description: Some("New blurb"),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(
            out.contains("<dc:description>New blurb</dc:description>"),
            "got: {out}"
        );
        assert!(!out.contains("Old blurb"), "old description leaked: {out}");
    }

    #[test]
    fn transform_replaces_dc_language() {
        let input = sample_epub3("");
        let target = Target {
            language: Some("fr"),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(out.contains("<dc:language>fr</dc:language>"), "got: {out}");
        assert!(
            !out.contains("<dc:language>en"),
            "old language leaked: {out}"
        );
    }

    #[test]
    fn transform_replaces_dc_publisher() {
        let input = sample_epub3(r"<dc:publisher>Old House</dc:publisher>");
        let target = Target {
            publisher: Some("New House"),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(
            out.contains("<dc:publisher>New House</dc:publisher>"),
            "got: {out}"
        );
        assert!(!out.contains("Old House"), "old publisher leaked: {out}");
    }

    #[test]
    fn transform_replaces_dc_date() {
        let input = sample_epub3(r"<dc:date>1990-01-01</dc:date>");
        let target = Target {
            pub_date: Some("2026-04-19"),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(out.contains("<dc:date>2026-04-19</dc:date>"), "got: {out}");
        assert!(!out.contains("1990-01-01"), "old date leaked: {out}");
    }

    #[test]
    fn transform_expands_self_closing_dc_title() {
        // `<dc:title/>` (empty element) must be rewritten as a paired
        // `<dc:title>NEW</dc:title>`.  Without this we'd leave the empty
        // element in place and drop the update silently.
        let input = r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="pub-id" xmlns:dc="http://purl.org/dc/elements/1.1/">
  <metadata>
    <dc:identifier id="pub-id">urn:uuid:A1B2C3D4</dc:identifier>
    <dc:title/>
    <dc:language>en</dc:language>
  </metadata>
  <manifest/>
  <spine/>
</package>"#;
        let target = Target {
            title: Some("Inserted"),
            ..Default::default()
        };
        let out = transform_str(input, &target);
        assert!(
            out.contains("<dc:title>Inserted</dc:title>"),
            "self-closing title was not expanded: {out}"
        );
    }

    #[test]
    fn transform_preserves_epub_version() {
        let input = sample_epub3("");
        let target = Target {
            title: Some("Anything"),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(
            out.contains(r#"<package version="3.0""#),
            "version stripped: {out}"
        );
    }

    #[test]
    fn transform_epub2_writes_calibre_series() {
        let input = sample_epub2("");
        let target = Target {
            series: Some(SeriesRef {
                name: "Mistborn",
                index: Some(1.0),
            }),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(
            out.contains(r#"name="calibre:series""#) && out.contains(r#"content="Mistborn""#),
            "calibre:series not inserted: {out}"
        );
        assert!(
            out.contains(r#"name="calibre:series_index""#) && out.contains(r#"content="1""#),
            "calibre:series_index not inserted: {out}"
        );
        // EPUB 2 should NOT grow a belongs-to-collection meta.
        assert!(
            !out.contains("belongs-to-collection"),
            "EPUB 2 grew belongs-to-collection: {out}"
        );
    }

    #[test]
    fn transform_epub3_writes_belongs_to_collection() {
        let input = sample_epub3("");
        let target = Target {
            series: Some(SeriesRef {
                name: "Mistborn",
                index: Some(1.0),
            }),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(
            out.contains(r#"property="belongs-to-collection""#),
            "belongs-to-collection not inserted: {out}"
        );
        assert!(out.contains("Mistborn"), "series name missing: {out}");
        assert!(
            out.contains(r#"property="collection-type""#),
            "collection-type refinement missing: {out}"
        );
        assert!(
            out.contains(r#"property="group-position""#),
            "group-position refinement missing: {out}"
        );
    }

    #[test]
    fn transform_updates_only_isbn_identifier() {
        let input =
            sample_epub3(r#"<dc:identifier opf:scheme="ISBN">9789999999999</dc:identifier>"#);
        let target = Target {
            isbn_13: Some("9781234567890"),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(out.contains("9781234567890"), "ISBN not updated: {out}");
        assert!(!out.contains("9789999999999"), "old ISBN leaked: {out}");
        // Package unique-identifier still points at UUID.
        assert!(
            out.contains(r#"unique-identifier="pub-id""#),
            "unique-identifier mutated: {out}"
        );
        // UUID identifier preserved.
        assert!(
            out.contains("urn:uuid:A1B2C3D4"),
            "UUID identifier lost: {out}"
        );
    }

    #[test]
    fn transform_inserts_isbn_when_absent() {
        let input = sample_epub3("");
        let target = Target {
            isbn_13: Some("9781234567890"),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(
            out.contains(r#"opf:scheme="ISBN""#),
            "ISBN identifier not inserted: {out}"
        );
        assert!(out.contains("9781234567890"), "ISBN value missing: {out}");
        // Pre-existing UUID identifier is still there.
        assert!(
            out.contains("urn:uuid:A1B2C3D4"),
            "UUID identifier lost: {out}"
        );
        assert!(
            out.contains(r#"unique-identifier="pub-id""#),
            "unique-identifier mutated: {out}"
        );
    }

    #[test]
    fn transform_preserves_custom_meta() {
        let input = sample_epub3(
            r#"<dc:coverage>2010</dc:coverage>
    <meta name="kobo:something" content="X"/>"#,
        );
        let target = Target {
            title: Some("New"),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(
            out.contains("<dc:coverage>2010</dc:coverage>"),
            "dc:coverage lost: {out}"
        );
        assert!(
            out.contains(r#"name="kobo:something""#) && out.contains(r#"content="X""#),
            "kobo:something meta lost: {out}"
        );
    }

    #[test]
    fn transform_epub3_with_existing_calibre_updates_both() {
        let input = sample_epub3(
            r##"<meta property="belongs-to-collection" id="c1">Old Series</meta>
    <meta refines="#c1" property="collection-type">series</meta>
    <meta name="calibre:series" content="Old Series"/>
    <meta name="calibre:series_index" content="1"/>"##,
        );
        let target = Target {
            series: Some(SeriesRef {
                name: "New Series",
                index: Some(2.0),
            }),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(out.contains("New Series"), "new series name missing: {out}");
        assert!(!out.contains("Old Series"), "old series leaked: {out}");
        // Both forms preserved in EPUB 3 when calibre:series was present.
        assert!(
            out.contains(r#"property="belongs-to-collection""#),
            "belongs-to-collection missing: {out}"
        );
        assert!(
            out.contains(r#"name="calibre:series""#),
            "calibre:series missing: {out}"
        );
    }
}

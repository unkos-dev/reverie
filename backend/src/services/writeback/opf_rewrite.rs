//! Pure-function OPF XML rewriter.
//!
//! Streams the source OPF as `quick_xml` events, replaces targeted Dublin
//! Core / `<meta>` elements in place, and preserves everything else
//! byte-for-byte (unknown namespaces, custom `<meta>`, `<dc:coverage>`,
//! prologue, declaration order).  Callers pass a `Target` carrying the
//! desired per-field values. ISBN and publication targets distinguish
//! unchanged values, replacements and clears.
//!
//! Memory-instinct: never use string substitution or regex on XML.  All
//! transforms go through `quick-xml` events.

use quick_xml::Reader;
use quick_xml::Writer;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::name::{Namespace, ResolveResult};
use quick_xml::reader::NsReader;
use std::collections::{HashMap, HashSet};
use std::io::Cursor;

use super::error::WritebackError;

/// Target values for existing metadata; ISBN and publication operations also insert or clear.
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
    /// `None` leaves publication metadata unchanged; `Some(None)` clears it;
    /// `Some(Some(value))` writes text at its supplied precision.
    pub pub_date: Option<Option<&'a str>>,
    /// `None` leaves ISBN-10 unchanged; `Some(None)` clears bibliographic entries.
    pub isbn_10: Option<Option<&'a str>>,
    /// `None` leaves ISBN-13 unchanged; `Some(None)` clears bibliographic entries.
    pub isbn_13: Option<Option<&'a str>>,
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

    let mut meta = scan_metadata(&events);
    scan_isbn_metadata(opf_bytes, &events, &mut meta)?;
    let operations = [target.isbn_10, target.isbn_13];
    let mut selected = [None; 2];
    let mut removed = HashSet::new();
    for (i, _) in events.iter().enumerate() {
        if let Some(&(format, _)) = meta.isbn_identifiers.get(&i) {
            match operations[format] {
                Some(Some(_)) if selected[format].is_none() => selected[format] = Some(i),
                Some(_) => {
                    removed.insert(i);
                }
                None => {}
            }
        }
    }
    let mut removed_ids = HashSet::new();
    loop {
        for &i in &removed {
            if let Event::Start(e) | Event::Empty(e) = &events[i]
                && let Some(id) = isbn_attribute(e, "id")?
            {
                removed_ids.insert(id);
            }
        }
        let count = removed.len();
        for (i, target) in &meta.refinements {
            if removed_ids.contains(target) {
                removed.insert(*i);
            }
        }
        if removed.len() == count {
            break;
        }
    }

    // Rewrite pass.
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut in_metadata_depth: Option<usize> = None;
    let mut depth: usize = 0;
    let mut belongs_to_written = false;
    let mut calibre_series_written = false;

    // If the file uses EPUB 3, we write belongs-to-collection.  If it ALSO
    // has pre-existing calibre:series meta, we update both (downstream-reader
    // compatibility).  If the file is EPUB 2, we write only calibre:series.
    let use_belongs_to = meta.epub_version.starts_with('3');
    let use_calibre = !meta.epub_version.starts_with('3') || meta.had_calibre_series;
    let use_belongs = use_belongs_to || meta.had_belongs_to_collection;

    let mut i = 0;
    while i < events.len() {
        let ev = &events[i];
        if removed.contains(&i) {
            i = skip_to_matching_end(&events, i) + 1;
            continue;
        }
        if let Some(&(format, had_urn)) = meta.isbn_identifiers.get(&i)
            && let Some(Some(value)) = operations[format]
            && let Event::Start(e) | Event::Empty(e) = ev
        {
            let mut start = e.clone();
            if attr_value(e, "id").is_none() {
                let id = allocate_isbn_id(&mut meta.document_ids, format);
                start.push_attribute(("id", id.as_str()));
            }
            let text = if had_urn || meta.epub_version.starts_with('3') {
                format!("urn:isbn:{value}")
            } else {
                value.to_owned()
            };
            write_replaced_element_from_empty(&mut writer, &start, &text)?;
            i = skip_to_matching_end(&events, i) + 1;
            continue;
        }
        match ev {
            Event::Start(e) => {
                depth += 1;
                let local = local_name(e.name().as_ref()).to_owned();

                if local == "metadata" && in_metadata_depth.is_none() {
                    in_metadata_depth = Some(depth);
                    writer.write_event(Event::Start(e.clone()))?;
                    i += 1;
                    continue;
                }

                if in_metadata_depth.is_some() {
                    if meta.publication_dates.contains(&i) {
                        match target.pub_date {
                            Some(None) => {
                                i = skip_to_matching_end(&events, i) + 1;
                                depth -= 1;
                                continue;
                            }
                            Some(Some(value)) if meta.selected_publication_date == Some(i) => {
                                write_replaced_element(&mut writer, e, value)?;
                                i = skip_to_matching_end(&events, i) + 1;
                                depth -= 1;
                                continue;
                            }
                            _ => {}
                        }
                    }
                    // Targeted DC elements: replace text.
                    if let Some(new_text) = target_text_for_dc(&local, e, target, &meta.title_types)
                    {
                        // Skip everything until the matching End tag.
                        write_replaced_element(&mut writer, e, new_text)?;
                        i = skip_to_matching_end(&events, i) + 1;
                        depth -= 1;
                        continue;
                    }
                    // <meta property="belongs-to-collection"> — update text + refinement metas.
                    if local == "meta"
                        && has_attr(e, "property", "belongs-to-collection")
                        && let Some(series) = target.series
                        && use_belongs
                    {
                        let id = attr_value(e, "id").unwrap_or_else(|| "series-1".to_owned());
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
                let local = local_name(e.name().as_ref()).to_owned();
                if (local == "metadata" && matches!(target.pub_date, Some(Some(_))))
                    || (meta.isbn_metadata.contains(&i)
                        && operations
                            .iter()
                            .any(|operation| matches!(operation, Some(Some(_)))))
                {
                    writer.write_event(Event::Start(e.clone()))?;
                    if local == "metadata"
                        && let Some(Some(value)) = target.pub_date
                    {
                        write_publication_date(&mut writer, value)?;
                    }
                    for (format, operation) in operations.iter().enumerate() {
                        if meta.isbn_metadata.contains(&i)
                            && let Some(Some(value)) = operation
                        {
                            let id = allocate_isbn_id(&mut meta.document_ids, format);
                            write_new_isbn_identifier(&mut writer, value, &id, &meta.epub_version)?;
                        }
                    }
                    writer.write_event(Event::End(BytesEnd::new(e.name().as_ref())))?;
                    i += 1;
                    continue;
                }
                if in_metadata_depth.is_some() {
                    if meta.publication_dates.contains(&i) {
                        match target.pub_date {
                            Some(None) => {
                                i += 1;
                                continue;
                            }
                            Some(Some(value)) if meta.selected_publication_date == Some(i) => {
                                write_replaced_element_from_empty(&mut writer, e, value)?;
                                i += 1;
                                continue;
                            }
                            _ => {}
                        }
                    }
                    if let Some(new_text) = target_text_for_dc(&local, e, target, &meta.title_types)
                    {
                        // Rare: <dc:title/> as empty element — expand to paired.
                        write_replaced_element_from_empty(&mut writer, e, new_text)?;
                        i += 1;
                        continue;
                    }
                    // <meta name="calibre:series" content="..."/>
                    if local == "meta"
                        && has_attr(e, "name", "calibre:series")
                        && let Some(series) = target.series
                        && use_calibre
                    {
                        write_calibre_series(&mut writer, series.name)?;
                        i += 1;
                        calibre_series_written = true;
                        continue;
                    }
                    if local == "meta"
                        && has_attr(e, "name", "calibre:series_index")
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
                depth -= 1;
                let local = local_name(e.name().as_ref()).to_owned();

                let closes_metadata = in_metadata_depth == Some(depth + 1) && local == "metadata";
                if closes_metadata
                    && meta.selected_publication_date.is_none()
                    && let Some(Some(value)) = target.pub_date
                {
                    write_publication_date(&mut writer, value)?;
                }
                for (format, operation) in operations.iter().enumerate() {
                    if meta.isbn_metadata.contains(&i)
                        && selected[format].is_none()
                        && let Some(Some(value)) = operation
                    {
                        let id = allocate_isbn_id(&mut meta.document_ids, format);
                        write_new_isbn_identifier(&mut writer, value, &id, &meta.epub_version)?;
                    }
                }
                if closes_metadata {
                    // belongs-to-collection insertion when absent.
                    if !belongs_to_written
                        && use_belongs_to
                        && let Some(series) = target.series
                    {
                        let id = "series-1";
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

fn write_publication_date(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    value: &str,
) -> Result<(), WritebackError> {
    let mut element = BytesStart::new("dc:date");
    element.push_attribute(("xmlns:dc", "http://purl.org/dc/elements/1.1/"));
    write_replaced_element_from_empty(writer, &element, value)
}

// ── Scan pass metadata ────────────────────────────────────────────────────

struct ScanResult {
    epub_version: String,
    had_belongs_to_collection: bool,
    had_calibre_series: bool,
    /// Declared `title-type` per refined element `id` (the `refines`
    /// attribute with its leading `#` stripped), lowercased.
    title_types: HashMap<String, String>,
    publication_dates: HashSet<usize>,
    selected_publication_date: Option<usize>,
    document_ids: HashSet<String>,
    isbn_metadata: HashSet<usize>,
    isbn_identifiers: HashMap<usize, (usize, bool)>,
    refinements: HashMap<usize, String>,
}

fn scan_metadata(events: &[Event<'static>]) -> ScanResult {
    let mut epub_version = "3.0".to_string();
    let mut had_belongs = false;
    let mut had_calibre = false;
    let mut title_types: HashMap<String, String> = HashMap::new();
    for (i, ev) in events.iter().enumerate() {
        match ev {
            Event::Start(e) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                if local == "package"
                    && let Some(v) = attr_value_as_string(e, "version")
                {
                    epub_version = v;
                }
                if local == "meta" && has_attr(e, "property", "belongs-to-collection") {
                    had_belongs = true;
                }
                // `<meta refines="#t2" property="title-type">subtitle</meta>`:
                // the value is the element's text body.
                if local == "meta"
                    && has_attr(e, "property", "title-type")
                    && let Some(refines) = attr_value(e, "refines")
                    && let Some(value) =
                        attr_value_as_string(e, "content").or_else(|| element_text(events, i))
                {
                    title_types.insert(strip_fragment(&refines), normalize_title_type(&value));
                }
            }
            Event::Empty(e) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                if local == "meta" && has_attr(e, "name", "calibre:series") {
                    had_calibre = true;
                }
                // Self-closing refine: the value lives in `content`, and
                // quick-xml never fires a Text event for it.
                if local == "meta"
                    && has_attr(e, "property", "title-type")
                    && let Some(refines) = attr_value(e, "refines")
                    && let Some(value) = attr_value_as_string(e, "content")
                {
                    title_types.insert(strip_fragment(&refines), normalize_title_type(&value));
                }
            }
            _ => {}
        }
    }
    let (publication_dates, selected_publication_date) =
        scan_publication_dates(events, &epub_version);
    ScanResult {
        document_ids: HashSet::new(),
        isbn_metadata: HashSet::new(),
        isbn_identifiers: HashMap::new(),
        refinements: HashMap::new(),
        publication_dates,
        selected_publication_date,
        epub_version,
        had_belongs_to_collection: had_belongs,
        had_calibre_series: had_calibre,
        title_types,
    }
}

fn scan_isbn_metadata(
    bytes: &[u8],
    events: &[Event<'static>],
    meta: &mut ScanResult,
) -> Result<(), WritebackError> {
    let mut reader = NsReader::from_reader(bytes);
    let mut depth = 0;
    let mut metadata_depth = None;
    let mut dc_metadata_depth = None;
    let mut dc_metadata = HashSet::new();
    let mut package_id = None;
    let mut candidates = Vec::new();
    let mut declared = HashSet::new();
    for (i, event) in events.iter().enumerate() {
        let (namespace, _) = reader.read_resolved_event()?;
        let is_opf = namespace == ResolveResult::Bound(Namespace("http://www.idpf.org/2007/opf"));
        if matches!(event, Event::End(_)) {
            if dc_metadata_depth == Some(depth) {
                dc_metadata.insert(i);
                dc_metadata_depth = None;
            }
            if metadata_depth == Some(depth) {
                meta.isbn_metadata.insert(i);
                metadata_depth = None;
            }
            depth -= 1;
            continue;
        }
        let (Event::Start(element) | Event::Empty(element)) = event else {
            continue;
        };
        let element_depth = depth + 1;
        depth += usize::from(matches!(event, Event::Start(_)));
        if let Some(id) = isbn_attribute(element, "id")? {
            meta.document_ids.insert(id);
        }
        let name = element.name();
        let local = local_name(name.as_ref());
        if is_opf && local == "package" && element_depth == 1 {
            package_id = isbn_attribute(element, "unique-identifier")?;
            meta.epub_version =
                isbn_attribute(element, "version")?.unwrap_or_else(|| meta.epub_version.clone());
        }
        if is_opf && local == "metadata" && element_depth == 2 {
            meta.isbn_metadata.insert(i);
            metadata_depth = matches!(event, Event::Start(_)).then_some(element_depth);
            continue;
        }
        if is_opf
            && local == "dc-metadata"
            && meta.epub_version.starts_with('2')
            && metadata_depth == Some(element_depth - 1)
        {
            dc_metadata.insert(i);
            dc_metadata_depth = matches!(event, Event::Start(_)).then_some(element_depth);
            continue;
        }
        if metadata_depth != Some(element_depth - 1) && dc_metadata_depth != Some(element_depth - 1)
        {
            continue;
        }
        scan_isbn_entry(&reader, events, i, meta, &mut candidates, &mut declared)?;
    }
    if !dc_metadata.is_empty() {
        meta.isbn_metadata = dc_metadata;
    }
    meta.isbn_identifiers =
        classify_isbn_identifiers(events, candidates, package_id.as_deref(), &declared)?;
    Ok(())
}

fn scan_isbn_entry(
    reader: &NsReader<&[u8]>,
    events: &[Event<'static>],
    index: usize,
    meta: &mut ScanResult,
    candidates: &mut Vec<(usize, Option<String>, bool)>,
    declared: &mut HashSet<String>,
) -> Result<(), WritebackError> {
    let (Event::Start(element) | Event::Empty(element)) = &events[index] else {
        return Ok(());
    };
    let (namespace, local) = reader.resolver().resolve_element(element.name());
    if namespace == ResolveResult::Bound(Namespace("http://purl.org/dc/elements/1.1/"))
        && local.as_ref() == "identifier"
    {
        candidates.push((
            index,
            isbn_attribute(element, "id")?,
            has_isbn_scheme(element, reader.resolver())?,
        ));
    }
    if namespace == ResolveResult::Bound(Namespace("http://www.idpf.org/2007/opf"))
        && local.as_ref() == "meta"
        && let Some(id) = scan_isbn_refinement(element, events, index, meta)?
    {
        declared.insert(id);
    }
    Ok(())
}

fn scan_isbn_refinement(
    element: &BytesStart<'_>,
    events: &[Event<'static>],
    index: usize,
    meta: &mut ScanResult,
) -> Result<Option<String>, WritebackError> {
    let Some(reference) = isbn_attribute(element, "refines")? else {
        return Ok(None);
    };
    let Some(id) = reference.strip_prefix('#') else {
        return Ok(None);
    };
    meta.refinements.insert(index, id.to_owned());
    if isbn_attribute(element, "property")?.as_deref() != Some("identifier-type") {
        return Ok(None);
    }
    let value = match isbn_attribute(element, "content")? {
        Some(value) => value,
        None => isbn_element_text(events, index)?,
    };
    let scheme = isbn_attribute(element, "scheme")?;
    let declared = value.trim().eq_ignore_ascii_case("isbn")
        || (scheme.as_deref() == Some("onix:codelist5") && matches!(value.trim(), "02" | "15"));
    Ok(declared.then(|| id.to_owned()))
}

fn classify_isbn_identifiers(
    events: &[Event<'static>],
    candidates: Vec<(usize, Option<String>, bool)>,
    package_id: Option<&str>,
    declared: &HashSet<String>,
) -> Result<HashMap<usize, (usize, bool)>, WritebackError> {
    let mut identifiers = HashMap::new();
    for (i, id, scheme_is_isbn) in candidates {
        if id.is_some() && id.as_deref() == package_id {
            continue;
        }
        let value = isbn_element_text(events, i)?;
        let value = value.trim();
        let urn = value
            .get(..9)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("urn:isbn:"));
        if !scheme_is_isbn && !urn && !id.as_ref().is_some_and(|id| declared.contains(id)) {
            continue;
        }
        let number = if urn { &value[9..] } else { value };
        let format = match number.parse::<isbn::Isbn>() {
            Ok(isbn::Isbn::_10(_)) => 0,
            Ok(isbn::Isbn::_13(number))
                if number.to_string().starts_with("978")
                    || number.to_string().starts_with("979") =>
            {
                1
            }
            _ => continue,
        };
        identifiers.insert(i, (format, urn));
    }
    Ok(identifiers)
}

fn has_isbn_scheme(
    element: &BytesStart<'_>,
    resolver: &quick_xml::name::NamespaceResolver,
) -> Result<bool, WritebackError> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(quick_xml::Error::from)?;
        let (binding, name) = resolver.resolve_attribute(attribute.key);
        if name.as_ref() == "scheme"
            && binding == ResolveResult::Bound(Namespace("http://www.idpf.org/2007/opf"))
            && attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                .eq_ignore_ascii_case("isbn")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn isbn_element_text(events: &[Event<'static>], start: usize) -> Result<String, WritebackError> {
    let mut value = String::new();
    if matches!(events[start], Event::Start(_)) {
        for event in &events[start + 1..skip_to_matching_end(events, start)] {
            match event {
                Event::Text(text) => {
                    value.push_str(&text.xml_content(quick_xml::XmlVersion::Implicit1_0));
                }
                Event::CData(text) => {
                    value.push_str(&text.xml_content(quick_xml::XmlVersion::Implicit1_0));
                }
                Event::GeneralRef(reference) => value.push_str(
                    &quick_xml::escape::unescape(&format!("&{};", reference.as_ref()))
                        .map_err(quick_xml::Error::from)?,
                ),
                _ => {}
            }
        }
    }
    Ok(value)
}

fn isbn_attribute(element: &BytesStart<'_>, name: &str) -> Result<Option<String>, WritebackError> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(quick_xml::Error::from)?;
        if attribute.key.as_ref() == name {
            return Ok(Some(
                attribute
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn scan_publication_dates(
    events: &[Event<'static>],
    epub_version: &str,
) -> (HashSet<usize>, Option<usize>) {
    let mut publication_dates = Vec::new();
    let mut labelled_dates = Vec::new();
    let mut in_metadata = false;
    for (i, ev) in events.iter().enumerate() {
        match ev {
            Event::Start(e) | Event::Empty(e)
                if local_name(e.name().as_ref()) == "date" && in_metadata =>
            {
                let event = attr_value(e, "event");
                if event.as_deref().is_none_or(|value| value == "publication") {
                    publication_dates.push(i);
                    if event.as_deref() == Some("publication") {
                        labelled_dates.push(i);
                    }
                }
            }
            Event::Start(e) if local_name(e.name().as_ref()) == "metadata" => in_metadata = true,
            Event::End(e) if local_name(e.name().as_ref()) == "metadata" => in_metadata = false,
            _ => {}
        }
    }
    let selected_publication_date = if epub_version.starts_with('3') {
        publication_dates.first().copied()
    } else {
        labelled_dates
            .first()
            .or_else(|| publication_dates.first())
            .copied()
    };
    (
        publication_dates.into_iter().collect(),
        selected_publication_date,
    )
}

/// Text body of the element starting at `start_idx`, empty text discarded.
///
/// A CDATA section is character data (XML 1.0 section 2.7), so it carries the
/// element's value just as a text node does; quick-xml merely reports it as a
/// separate event.  Missing that form here would leave the refine unrecorded
/// and route the refined `dc:title` back through the main-title rewrite.
fn element_text(events: &[Event<'static>], start_idx: usize) -> Option<String> {
    let end = skip_to_matching_end(events, start_idx);
    events[start_idx + 1..end.min(events.len())]
        .iter()
        .find_map(|ev| match ev {
            Event::Text(t) => Some(t.trim().to_string()),
            Event::CData(c) => Some(c.trim().to_string()),
            _ => None,
        })
        .filter(|s| !s.is_empty())
}

/// `refines="#t2"` addresses the element whose `id` is `t2`.
fn strip_fragment(refines: &str) -> String {
    refines.strip_prefix('#').unwrap_or(refines).to_owned()
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
    let name = start.name().as_ref().to_owned();
    writer.write_event(Event::Start(start.clone()))?;
    writer.write_event(Event::Text(BytesText::new(new_text)))?;
    writer.write_event(Event::End(BytesEnd::new(name)))?;
    Ok(())
}

fn write_replaced_element_from_empty(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    empty: &BytesStart<'_>,
    new_text: &str,
) -> Result<(), WritebackError> {
    let name = empty.name().as_ref().to_owned();
    let mut start = BytesStart::new(name.clone());
    for attr in empty.attributes().flatten() {
        start.push_attribute(attr);
    }
    writer.write_event(Event::Start(start))?;
    writer.write_event(Event::Text(BytesText::new(new_text)))?;
    writer.write_event(Event::End(BytesEnd::new(name)))?;
    Ok(())
}

fn write_new_isbn_identifier(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    isbn: &str,
    id: &str,
    version: &str,
) -> Result<(), WritebackError> {
    let mut start = BytesStart::new("dc:identifier");
    start.push_attribute(("xmlns:dc", "http://purl.org/dc/elements/1.1/"));
    start.push_attribute(("id", id));
    if version.starts_with('3') {
        write_replaced_element_from_empty(writer, &start, &format!("urn:isbn:{isbn}"))
    } else {
        start.push_attribute(("xmlns:opf", "http://www.idpf.org/2007/opf"));
        start.push_attribute(("opf:scheme", "ISBN"));
        write_replaced_element_from_empty(writer, &start, isbn)
    }
}

fn allocate_isbn_id(ids: &mut HashSet<String>, format: usize) -> String {
    let prefix = if format == 0 { "isbn-10" } else { "isbn-13" };
    let mut id = prefix.to_owned();
    let mut suffix = 1;
    while !ids.insert(id.clone()) {
        id = format!("{prefix}-{suffix}");
        suffix += 1;
    }
    id
}

fn write_belongs_to_collection(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    id: &str,
    name: &str,
) -> Result<(), WritebackError> {
    let mut start = BytesStart::new("meta");
    start.push_attribute(("property", "belongs-to-collection"));
    start.push_attribute(("id", id));
    writer.write_event(Event::Start(start))?;
    writer.write_event(Event::Text(BytesText::new(name)))?;
    writer.write_event(Event::End(BytesEnd::new("meta")))?;
    Ok(())
}

fn write_belongs_to_collection_refinements(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    id: &str,
    index: Option<f64>,
) -> Result<(), WritebackError> {
    let refines_target = format!("#{id}");
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
    local: &str,
    el: &BytesStart<'_>,
    target: &'t Target<'_>,
    title_types: &HashMap<String, String>,
) -> Option<&'t str> {
    match local {
        "title" => match title_type_of(el, title_types) {
            Some("subtitle") => target.subtitle,
            Some(other) if other != "main" => None,
            _ => target.title,
        },
        "description" => target.description,
        "language" => target.language,
        "publisher" => target.publisher,
        _ => None,
    }
}

/// The `title-type` refine declared for `el`, looked up by the element's `id`.
fn title_type_of<'m>(
    el: &BytesStart<'_>,
    title_types: &'m HashMap<String, String>,
) -> Option<&'m str> {
    let id = attr_value(el, "id")?;
    title_types.get(&id).map(String::as_str)
}

fn has_attr(start: &BytesStart<'_>, name: &str, value: &str) -> bool {
    for attr in start.attributes().flatten() {
        if local_name(attr.key.as_ref()) == name && attr.value.as_ref().eq_ignore_ascii_case(value)
        {
            return true;
        }
    }
    false
}

fn attr_value(start: &BytesStart<'_>, name: &str) -> Option<String> {
    for attr in start.attributes().flatten() {
        if local_name(attr.key.as_ref()) == name {
            return Some(attr.value.into_owned());
        }
    }
    None
}

fn attr_value_as_string(start: &BytesStart<'_>, name: &str) -> Option<String> {
    attr_value(start, name)
}

fn local_name(name: &str) -> &str {
    name.split_once(':').map_or(name, |(_, local)| local)
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
        Event::Start(e) => e.name().as_ref().to_owned(),
        _ => return start_idx,
    };
    let mut depth = 1usize;
    let mut i = start_idx + 1;
    while i < events.len() {
        match &events[i] {
            Event::Start(e) if e.name().as_ref() == start_name.as_str() => {
                depth += 1;
            }
            Event::End(e) if e.name().as_ref() == start_name.as_str() => {
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
    fn transform_preserves_cdata_subtitle_without_canonical_subtitle() {
        // Same shape with no canonical subtitle to write: the declared
        // subtitle must survive untouched rather than take the main title.
        let input = sample_epub3_titles(
            r##"<dc:title id="t1">Old Title</dc:title>
    <dc:title id="t2">Old Subtitle</dc:title>
    <meta refines="#t2" property="title-type"><![CDATA[ subtitle ]]></meta>"##,
        );
        let target = Target {
            title: Some("New Title"),
            ..Default::default()
        };
        let out = transform_str(&input, &target);
        assert!(
            out.contains(r#"<dc:title id="t2">Old Subtitle</dc:title>"#),
            "declared subtitle was destroyed: {out}"
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
    fn publication_date_writeback_preserves_intent() {
        for epub2 in [false, true] {
            for value in ["2024-06", "2024-06-15T12:30:59.123456789123+05:30"] {
                let metadata = "<dc:date opf:event=\"creation\">1900</dc:date><dc:date>2000</dc:date><dc:date opf:event=\"publication\">2001</dc:date><dc:date opf:event=\"modification\">2002</dc:date><meta property=\"dcterms:modified\">2020-01-01T00:00:00Z</meta>";
                let input = if epub2 {
                    sample_epub2(metadata)
                } else {
                    sample_epub3(metadata)
                };
                let unchanged = transform_str(&input, &Target::default());
                assert!(unchanged.contains(metadata));
                let set = transform_str(
                    &input,
                    &Target {
                        pub_date: Some(Some(value)),
                        ..Default::default()
                    },
                );
                if epub2 {
                    assert!(set.contains(&format!("opf:event=\"publication\">{value}</dc:date>")));
                    assert!(set.contains("<dc:date>2000</dc:date>"));
                } else {
                    assert!(set.contains(&format!("<dc:date>{value}</dc:date>")));
                }
                let clear = transform_str(
                    &input,
                    &Target {
                        pub_date: Some(None),
                        ..Default::default()
                    },
                );
                for output in [&set, &clear] {
                    assert!(output.contains("opf:event=\"creation\">1900</dc:date>"));
                    assert!(output.contains("opf:event=\"modification\">2002</dc:date>"));
                    assert!(
                        output
                            .contains("property=\"dcterms:modified\">2020-01-01T00:00:00Z</meta>")
                    );
                }
                assert!(!clear.contains("<dc:date>2000</dc:date>"));
                assert!(!clear.contains("opf:event=\"publication\""));
                let absent = if epub2 {
                    sample_epub2("")
                } else {
                    sample_epub3("")
                };
                let inserted = transform_str(
                    &absent,
                    &Target {
                        pub_date: Some(Some(value)),
                        ..Default::default()
                    },
                );
                assert!(inserted.contains(&format!(">{value}</dc:date>")));
                let empty = absent
                    .replace("<metadata>", "<metadata/><!--")
                    .replace("</metadata>", "-->");
                let inserted = transform_str(
                    &empty,
                    &Target {
                        pub_date: Some(Some(value)),
                        ..Default::default()
                    },
                );
                assert!(inserted.contains(&format!(">{value}</dc:date>")));
            }
        }
    }

    fn assert_isbn_writeback_state(xml: &str, expected: &[&str], version: &str) {
        let events = read_all_events(xml.as_bytes()).unwrap();
        let mut ids = HashSet::new();
        let mut identifiers = HashMap::new();
        let mut references = Vec::new();
        for (i, event) in events.iter().enumerate() {
            let (Event::Start(e) | Event::Empty(e)) = event else {
                continue;
            };
            if let Some(id) = attr_value(e, "id") {
                assert!(ids.insert(id.clone()), "duplicate document ID");
                if local_name(e.name().as_ref()) == "identifier" {
                    identifiers.insert(id, element_text(&events, i).unwrap());
                }
            }
            if let Some(reference) = attr_value(e, "refines") {
                references.push(reference);
            }
            if local_name(e.name().as_ref()) == "package" {
                assert_eq!(
                    attr_value(e, "unique-identifier").as_deref(),
                    Some("pub-id")
                );
                assert_eq!(attr_value(e, "version").as_deref(), Some(version));
            }
        }
        assert!(
            references
                .iter()
                .all(|reference| ids.contains(reference.strip_prefix('#').unwrap()))
        );
        for (id, value) in [
            ("pub-id", "9780306406157"),
            ("uuid", "urn:uuid:A1B2C3D4"),
            ("provider", "080442957X"),
            ("isbn-10", "080442957X"),
        ] {
            assert_eq!(
                identifiers.remove(id).as_deref(),
                Some(value),
                "identifier {id} changed"
            );
        }
        let mut actual: Vec<_> = identifiers
            .values()
            .map(|value| value.strip_prefix("urn:isbn:").unwrap_or(value))
            .collect();
        actual.sort_unstable();
        assert_eq!(actual, expected);
        if expected.contains(&"9791090636071") {
            assert_eq!(
                identifiers.get("editable").map(String::as_str),
                Some("urn:isbn:9791090636071")
            );
        } else {
            assert!(!ids.contains("kind") && !ids.contains("dependent"));
        }
        assert!(xml.contains("<dc:coverage>2010</dc:coverage>"));
        assert!(xml.contains(r##"<meta refines="#pub-id" property="identifier-type" scheme="onix:codelist5">15</meta>"##));
    }

    fn assert_wrapped_isbns(xml: &str, expected: &[&str]) -> HashMap<String, String> {
        let events = read_all_events(xml.as_bytes()).unwrap();
        let mut reader = NsReader::from_reader(xml.as_bytes());
        let mut parents = Vec::new();
        let mut wrapper = None;
        let mut identifiers = HashMap::new();
        for (i, event) in events.iter().enumerate() {
            let (namespace, _) = reader.read_resolved_event().unwrap();
            match event {
                Event::Start(element) | Event::Empty(element) => {
                    let name = element.name();
                    let local = local_name(name.as_ref());
                    if namespace == ResolveResult::Bound(Namespace("http://www.idpf.org/2007/opf"))
                        && local == "dc-metadata"
                    {
                        assert!(wrapper.replace(i).is_none());
                    }
                    if namespace
                        == ResolveResult::Bound(Namespace("http://purl.org/dc/elements/1.1/"))
                    {
                        assert_eq!(
                            parents.last().copied(),
                            wrapper,
                            "Dublin Core element outside wrapper"
                        );
                        if local == "identifier" {
                            let id = attr_value(element, "id").unwrap();
                            assert!(
                                identifiers
                                    .insert(id, element_text(&events, i).unwrap())
                                    .is_none()
                            );
                        }
                    }
                    if matches!(event, Event::Start(_)) {
                        parents.push(i);
                    }
                }
                Event::End(_) => {
                    parents.pop().unwrap();
                }
                _ => {}
            }
        }
        assert!(wrapper.is_some());
        assert_eq!(
            identifiers.remove("bookid").as_deref(),
            Some("urn:uuid:1234")
        );
        let mut actual: Vec<_> = identifiers.values().map(String::as_str).collect();
        actual.sort_unstable();
        assert_eq!(actual, expected);
        assert!(xml.contains(r#"unique-identifier="bookid""#));
        identifiers
    }

    #[test]
    fn isbn_writeback_contract() {
        // Data corruption boundary: bibliographic edits preserve package identity and unrelated XML.
        let input = sample_epub3(r##"<dc:identifier id="uuid">urn:uuid:A1B2C3D4</dc:identifier>
    <dc:identifier id="editable">urn:isbn:9780306406&#49;57</dc:identifier>
    <meta id="kind" refines="#editable" property="identifier-type" scheme="onix:codelist5">15</meta>
    <meta id="dependent" refines="#kind" property="alternate-script">ISBN</meta>
    <meta refines="#dependent" property="alternate-script" content="ISBN"/>
    <meta refines="#pub-id" property="identifier-type" scheme="onix:codelist5">15</meta>
    <dc:identifier id="duplicate" opf:scheme="ISBN">9780306406157</dc:identifier>
    <meta refines="#duplicate" property="identifier-type" content="ISBN"/>
    <dc:identifier id="provider">080442957X</dc:identifier>
    <vendor:identifier xmlns:vendor="urn:vendor" id="isbn-10" opf:scheme="ISBN">080442957X</vendor:identifier>
    <dc:coverage>2010</dc:coverage>"##)
            .replace("<dc:identifier id=\"pub-id\">urn:uuid:A1B2C3D4", "<dc:identifier id=\"pub-id\" opf:scheme=\"ISBN\">9780306406157");
        let mut output = input;
        let ten = "080442957X";
        let thirteen = "9791090636071";
        let changed_ten = "0306406152";
        for (index, (isbn_10, isbn_13, expected)) in [
            (Some(Some(ten)), Some(Some(thirteen)), vec![ten, thirteen]),
            (Some(Some(ten)), Some(Some(thirteen)), vec![ten, thirteen]),
            (Some(Some(changed_ten)), None, vec![changed_ten, thirteen]),
            (None, Some(None), vec![changed_ten]),
            (Some(None), None, vec![]),
            (None, Some(Some("9780306406157")), vec!["9780306406157"]),
        ]
        .into_iter()
        .enumerate()
        {
            let next = transform_str(
                &output,
                &Target {
                    isbn_10,
                    isbn_13,
                    ..Default::default()
                },
            );
            assert_isbn_writeback_state(&next, &expected, "3.0");
            if index == 1 {
                assert_eq!(next, output);
            }
            output = next;
        }
        let epub2 = output.replace("version=\"3.0\"", "version=\"2.0\"");
        let target = Target {
            isbn_13: Some(Some("9780804429573")),
            ..Default::default()
        };
        let set = transform_str(&epub2, &target);
        let clear = transform_str(
            &set,
            &Target {
                isbn_13: Some(None),
                ..Default::default()
            },
        );
        assert_isbn_writeback_state(&clear, &[], "2.0");
        let replay = transform_str(&set, &target);
        assert_isbn_writeback_state(&replay, &["9780804429573"], "2.0");
        assert_eq!(replay, set);
        let wrapped = sample_epub2(
            r#"<dc:identifier id="wrapped-isbn" opf:scheme="ISBN">9780306406157</dc:identifier>"#,
        )
        .replace("<metadata>", "<metadata><opf:dc-metadata>")
        .replace("</metadata>", "</opf:dc-metadata></metadata>");
        let clear = transform_str(
            &wrapped,
            &Target {
                isbn_13: Some(None),
                ..Default::default()
            },
        );
        assert_wrapped_isbns(&clear, &[]);
        let set = transform_str(
            &wrapped,
            &Target {
                isbn_10: Some(Some(ten)),
                isbn_13: Some(Some(thirteen)),
                ..Default::default()
            },
        );
        let identifiers = assert_wrapped_isbns(&set, &[ten, thirteen]);
        assert_eq!(
            identifiers.get("wrapped-isbn").map(String::as_str),
            Some(thirteen)
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
}

//! Cover-image replacement / insertion planning.
//!
//! Given the source OPF + the new cover bytes, [`plan_embed`] returns:
//! - binary entry replacements (existing cover manifest item → new bytes)
//! - additions (new ZIP entries for the insertion case)
//! - an OPF replacement reflecting any manifest changes
//!
//! Memory-instinct: when the new cover's format differs from the existing
//! cover's media-type, we write the new binary under a fresh entry name
//! and rewrite the manifest; the old entry stays in the archive as orphan
//! bytes — accepted MVP trade-off (Step 11 sweep can repack to drop orphans).

use std::collections::HashMap;
use std::io::Cursor;

use quick_xml::Reader;
use quick_xml::Writer;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use zip::write::{ExtendedFileOptions, FileOptions};

use super::error::WritebackError;

/// The planning output: what to mutate inside the repack helper.
#[derive(Debug)]
pub struct CoverPlan {
    pub binary_replacements: HashMap<String, Vec<u8>>,
    pub additions: Vec<(String, Vec<u8>, FileOptions<'static, ExtendedFileOptions>)>,
    pub opf_replacement: Option<Vec<u8>>,
}

/// Plan an embed of `new_cover_bytes` into the EPUB whose OPF is
/// `opf_bytes`.  Returns the repack inputs for the helper.
pub fn plan_embed(opf_bytes: &[u8], new_cover_bytes: &[u8]) -> Result<CoverPlan, WritebackError> {
    let fmt = image::guess_format(new_cover_bytes)
        .map_err(|e| WritebackError::ValidationRegressed(format!("unknown cover format: {e}")))?;
    let (media_type, ext, compression) = media_for(fmt);

    let scan = scan_opf(opf_bytes);
    let mut binary_replacements = HashMap::new();
    let mut additions: Vec<(String, Vec<u8>, FileOptions<'static, ExtendedFileOptions>)> =
        Vec::new();
    let mut opf_replacement: Option<Vec<u8>> = None;

    match scan.cover_href.clone() {
        Some(existing_href) => {
            let same_media = scan
                .cover_media_type
                .as_deref()
                .is_some_and(|m| m.eq_ignore_ascii_case(media_type));
            if same_media {
                binary_replacements.insert(existing_href, new_cover_bytes.to_vec());
            } else {
                // Format changed: add a new entry under a fresh name and
                // rewrite the OPF manifest to reference it.
                let new_href = format!("images/cover-image-writeback.{ext}");
                additions.push((
                    new_href.clone(),
                    new_cover_bytes.to_vec(),
                    FileOptions::default().compression_method(compression),
                ));
                opf_replacement = Some(rewrite_opf_cover_reference(
                    opf_bytes, &scan, &new_href, media_type,
                )?);
            }
        }
        None => {
            // No cover: insert a new manifest item + cover marker.
            let new_href = format!("images/cover-image-writeback.{ext}");
            additions.push((
                new_href.clone(),
                new_cover_bytes.to_vec(),
                FileOptions::default().compression_method(compression),
            ));
            opf_replacement = Some(insert_opf_cover(opf_bytes, &new_href, media_type, &scan)?);
        }
    }

    Ok(CoverPlan {
        binary_replacements,
        additions,
        opf_replacement,
    })
}

// ── OPF scan ──────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct OpfScan {
    epub_version: String,
    /// manifest item id → href
    manifest: HashMap<String, (String, Option<String>)>,
    /// `<meta name="cover" content="X"/>` → X
    cover_meta_content: Option<String>,
    /// resolved cover href (manifest lookup of cover_meta_content or properties=cover-image)
    cover_href: Option<String>,
    cover_media_type: Option<String>,
    /// manifest item id of the cover (for OPF-rewriting)
    cover_item_id: Option<String>,
}

fn scan_opf(opf_bytes: &[u8]) -> OpfScan {
    let mut scan = OpfScan {
        epub_version: "3.0".into(),
        ..Default::default()
    };
    let mut reader = Reader::from_reader(opf_bytes);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Err(_) | Ok(Event::Eof) => break,
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref()).to_vec();
                if local == b"package"
                    && let Some(v) = attr_str(&e, b"version")
                {
                    scan.epub_version = v;
                }
                if local == b"item" {
                    let id = attr_str(&e, b"id");
                    let href = attr_str(&e, b"href");
                    let media = attr_str(&e, b"media-type");
                    let props = attr_str(&e, b"properties");
                    if let (Some(id), Some(href)) = (id.clone(), href.clone()) {
                        scan.manifest
                            .insert(id.clone(), (href.clone(), media.clone()));
                        if props
                            .as_deref()
                            .is_some_and(|p| p.split_ascii_whitespace().any(|s| s == "cover-image"))
                        {
                            scan.cover_item_id = Some(id);
                            scan.cover_href = Some(href);
                            scan.cover_media_type = media;
                        }
                    }
                }
                if local == b"meta"
                    && attr_str(&e, b"name")
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case("cover"))
                {
                    scan.cover_meta_content = attr_str(&e, b"content");
                }
            }
            _ => {}
        }
        buf.clear();
    }

    // Resolve EPUB 2-style cover reference via meta+manifest lookup.
    if scan.cover_href.is_none()
        && let Some(content_id) = scan.cover_meta_content.clone()
        && let Some((href, media)) = scan.manifest.get(&content_id).cloned()
    {
        scan.cover_item_id = Some(content_id);
        scan.cover_href = Some(href);
        scan.cover_media_type = media;
    }
    // Final fallback: manifest id "cover-image" or "cover".
    if scan.cover_href.is_none() {
        for candidate in ["cover-image", "cover"] {
            if let Some((href, media)) = scan.manifest.get(candidate).cloned() {
                scan.cover_item_id = Some(candidate.into());
                scan.cover_href = Some(href);
                scan.cover_media_type = media;
                break;
            }
        }
    }

    scan
}

fn attr_str(start: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    for attr in start.attributes().flatten() {
        if local_name(attr.key.as_ref()) == name {
            return Some(String::from_utf8_lossy(&attr.value).into_owned());
        }
    }
    None
}

fn local_name(name: &[u8]) -> &[u8] {
    match name.iter().position(|&b| b == b':') {
        Some(pos) => &name[pos + 1..],
        None => name,
    }
}

// ── OPF rewriters ─────────────────────────────────────────────────────────

fn rewrite_opf_cover_reference(
    opf_bytes: &[u8],
    scan: &OpfScan,
    new_href: &str,
    new_media_type: &str,
) -> Result<Vec<u8>, WritebackError> {
    let cover_id = scan.cover_item_id.clone();
    let mut reader = Reader::from_reader(opf_bytes);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut buf = Vec::new();

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(WritebackError::Xml)?
        {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e)
                if local_name(e.name().as_ref()) == b"item"
                    && cover_id.as_deref() == attr_str(&e, b"id").as_deref() =>
            {
                let mut new_el =
                    BytesStart::new(String::from_utf8_lossy(e.name().as_ref()).to_string());
                for attr in e.attributes().flatten() {
                    let key_local = local_name(attr.key.as_ref());
                    let new_val = match key_local {
                        b"href" => new_href.as_bytes().to_vec(),
                        b"media-type" => new_media_type.as_bytes().to_vec(),
                        _ => attr.value.into_owned(),
                    };
                    new_el.push_attribute((attr.key.as_ref(), new_val.as_slice()));
                }
                writer
                    .write_event(Event::Empty(new_el))
                    .map_err(WritebackError::Io)?;
            }
            ev => writer.write_event(ev).map_err(WritebackError::Io)?,
        }
        buf.clear();
    }
    Ok(writer.into_inner().into_inner())
}

fn insert_opf_cover(
    opf_bytes: &[u8],
    new_href: &str,
    new_media_type: &str,
    scan: &OpfScan,
) -> Result<Vec<u8>, WritebackError> {
    let mut reader = Reader::from_reader(opf_bytes);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut buf = Vec::new();
    let is_epub3 = scan.epub_version.starts_with('3');

    let mut saw_metadata_open = false;
    let mut saw_manifest_open = false;

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(WritebackError::Xml)?
        {
            Event::Eof => break,
            Event::Start(e) if local_name(e.name().as_ref()) == b"metadata" => {
                saw_metadata_open = true;
                writer
                    .write_event(Event::Start(e.into_owned()))
                    .map_err(WritebackError::Io)?;
            }
            Event::Empty(e) if local_name(e.name().as_ref()) == b"metadata" => {
                // Self-closing <metadata/>: open it, inject the EPUB-2 cover
                // meta if needed, then close — otherwise we lose the insert site.
                let name = e.name().as_ref().to_vec();
                let mut start = BytesStart::new(String::from_utf8_lossy(&name).to_string());
                for attr in e.attributes().flatten() {
                    start.push_attribute(attr);
                }
                writer
                    .write_event(Event::Start(start))
                    .map_err(WritebackError::Io)?;
                if !is_epub3 {
                    let mut m = BytesStart::new("meta");
                    m.push_attribute(("name", "cover"));
                    m.push_attribute(("content", "cover-image"));
                    writer
                        .write_event(Event::Empty(m))
                        .map_err(WritebackError::Io)?;
                }
                writer
                    .write_event(Event::End(BytesEnd::new(
                        String::from_utf8_lossy(&name).to_string(),
                    )))
                    .map_err(WritebackError::Io)?;
            }
            Event::End(e) if local_name(e.name().as_ref()) == b"metadata" => {
                // For EPUB 2, insert <meta name="cover" content="cover-image"/> here.
                if saw_metadata_open && !is_epub3 {
                    let mut m = BytesStart::new("meta");
                    m.push_attribute(("name", "cover"));
                    m.push_attribute(("content", "cover-image"));
                    writer
                        .write_event(Event::Empty(m))
                        .map_err(WritebackError::Io)?;
                }
                writer
                    .write_event(Event::End(e.into_owned()))
                    .map_err(WritebackError::Io)?;
            }
            Event::Start(e) if local_name(e.name().as_ref()) == b"manifest" => {
                saw_manifest_open = true;
                writer
                    .write_event(Event::Start(e.into_owned()))
                    .map_err(WritebackError::Io)?;
                // Insert new cover manifest item immediately after the opening tag.
                let mut item = BytesStart::new("item");
                item.push_attribute(("id", "cover-image"));
                item.push_attribute(("href", new_href));
                item.push_attribute(("media-type", new_media_type));
                if is_epub3 {
                    item.push_attribute(("properties", "cover-image"));
                }
                writer
                    .write_event(Event::Empty(item))
                    .map_err(WritebackError::Io)?;
            }
            ev => writer.write_event(ev).map_err(WritebackError::Io)?,
        }
        buf.clear();
    }

    if !saw_manifest_open {
        return Err(WritebackError::ValidationRegressed(
            "opf has no <manifest>".into(),
        ));
    }
    let _ = saw_metadata_open;
    let _ = BytesText::new(""); // silence unused import
    let _ = BytesEnd::new(""); // silence unused import
    Ok(writer.into_inner().into_inner())
}

fn media_for(fmt: image::ImageFormat) -> (&'static str, &'static str, zip::CompressionMethod) {
    match fmt {
        image::ImageFormat::Jpeg => ("image/jpeg", "jpg", zip::CompressionMethod::Stored),
        image::ImageFormat::Png => ("image/png", "png", zip::CompressionMethod::Deflated),
        image::ImageFormat::WebP => ("image/webp", "webp", zip::CompressionMethod::Stored),
        // Default to png for anything else (image crate mostly returns above three).
        _ => ("image/png", "png", zip::CompressionMethod::Deflated),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tiny valid PNG: 1x1 black pixel.
    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn epub3_opf_with_cover() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid">
  <metadata/>
  <manifest>
    <item id="cover-image" href="images/old-cover.png" media-type="image/png" properties="cover-image"/>
    <item id="chap1" href="c1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="chap1"/></spine>
</package>"#
            .to_string()
    }

    fn epub2_opf_with_cover() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="2.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid" xmlns:opf="http://www.idpf.org/2007/opf">
  <metadata>
    <meta name="cover" content="cover-img"/>
  </metadata>
  <manifest>
    <item id="cover-img" href="images/old-cover.jpg" media-type="image/jpeg"/>
    <item id="chap1" href="c1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="chap1"/></spine>
</package>"#
            .to_string()
    }

    fn epub3_opf_no_cover() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid">
  <metadata/>
  <manifest>
    <item id="chap1" href="c1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="chap1"/></spine>
</package>"#
            .to_string()
    }

    fn epub2_opf_no_cover() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="2.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid" xmlns:opf="http://www.idpf.org/2007/opf">
  <metadata/>
  <manifest>
    <item id="chap1" href="c1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="chap1"/></spine>
</package>"#
            .to_string()
    }

    #[test]
    fn plan_replaces_existing_epub3_same_media() {
        let opf = epub3_opf_with_cover();
        let plan = plan_embed(opf.as_bytes(), PNG_1X1).unwrap();
        assert!(
            plan.binary_replacements
                .contains_key("images/old-cover.png")
        );
        assert!(plan.additions.is_empty());
        // Same media-type → no OPF rewrite needed.
        assert!(plan.opf_replacement.is_none());
    }

    #[test]
    fn plan_inserts_when_absent_epub3() {
        let opf = epub3_opf_no_cover();
        let plan = plan_embed(opf.as_bytes(), PNG_1X1).unwrap();
        assert!(plan.binary_replacements.is_empty());
        assert_eq!(plan.additions.len(), 1);
        let opf_new = String::from_utf8(plan.opf_replacement.unwrap()).unwrap();
        assert!(
            opf_new.contains(r#"id="cover-image""#)
                && opf_new.contains(r#"properties="cover-image""#),
            "EPUB 3 manifest missing cover-image: {opf_new}"
        );
    }

    #[test]
    fn plan_inserts_when_absent_epub2() {
        let opf = epub2_opf_no_cover();
        let plan = plan_embed(opf.as_bytes(), PNG_1X1).unwrap();
        assert_eq!(plan.additions.len(), 1);
        let opf_new = String::from_utf8(plan.opf_replacement.unwrap()).unwrap();
        assert!(
            opf_new.contains(r#"name="cover""#),
            "EPUB 2 missing <meta name=cover>: {opf_new}"
        );
        assert!(
            opf_new.contains(r#"id="cover-image""#),
            "EPUB 2 manifest missing cover-image: {opf_new}"
        );
    }

    #[test]
    fn plan_replaces_existing_epub2_same_media() {
        let opf = epub2_opf_with_cover();
        // New cover is PNG, old was JPEG → different media-type.
        let plan = plan_embed(opf.as_bytes(), PNG_1X1).unwrap();
        assert!(plan.binary_replacements.is_empty());
        assert_eq!(plan.additions.len(), 1);
        let opf_new = String::from_utf8(plan.opf_replacement.unwrap()).unwrap();
        assert!(
            opf_new.contains("image/png"),
            "media-type not updated: {opf_new}"
        );
    }
}

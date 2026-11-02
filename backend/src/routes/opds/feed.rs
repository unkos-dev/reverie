//! OPDS 1.2 Atom XML feed builder.
//!
//! Pure, stateless helper — no DB access, no I/O. Callers build an
//! [`AcquisitionEntry`] per row and feed it through [`FeedBuilder`].
//! Everything a client sees that originates from user data flows through
//! [`super::xml::sanitise_xml_text`] first, and through quick-xml's
//! `BytesText::new` / `push_attribute` auto-escaping on write.
//!
//! Namespaces: OPDS 1.2 uses the default (unprefixed) namespace for Atom
//! elements. Only `opds:`, `dc:`, and `opensearch:` are explicitly prefixed.
//! Do NOT declare `xmlns:atom` — treat Atom as the default.
//!
//! # Why `expect_used` is allowed here
//!
//! Every `expect()` call in this module writes to a `Writer<Cursor<Vec<u8>>>`.
//! `std::io::Cursor<Vec<u8>>` is an infallible sink — there is no I/O and
//! writes cannot fail except on OOM (which is a process abort, not a Result).
//! Making every builder method return `Result` would cascade error-handling
//! into every call site for an error path that physically cannot occur.
#![expect(
    clippy::expect_used,
    reason = "all expects write to Cursor<Vec<u8>>, an infallible sink"
)]

use chrono::{DateTime, SecondsFormat, Utc};
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use std::io::Cursor;
use url::Url;
use uuid::Uuid;

use super::xml::sanitise_xml_text;

/// Default Atom namespace URI (OPDS 1.2 leaves it unprefixed).
pub const ATOM_NS: &str = "http://www.w3.org/2005/Atom";
/// OPDS catalog namespace URI (`opds:` prefix).
pub const OPDS_NS: &str = "http://opds-spec.org/2010/catalog";
/// Dublin Core terms namespace URI (`dc:` prefix).
pub const DC_NS: &str = "http://purl.org/dc/terms/";
/// `OpenSearch` 1.1 namespace URI (`opensearch:` prefix).
pub const OPENSEARCH_NS: &str = "http://a9.com/-/spec/opensearch/1.1/";

/// `Content-Type` for navigation feeds (carries the `kind=navigation`
/// profile parameter).
pub const NAVIGATION_TYPE: &str = "application/atom+xml;profile=opds-catalog;kind=navigation";
/// `Content-Type` for acquisition feeds (carries the `kind=acquisition`
/// profile parameter).
pub const ACQUISITION_TYPE: &str = "application/atom+xml;profile=opds-catalog;kind=acquisition";

/// `rel` value for the EPUB acquisition link on each entry.
pub const REL_ACQUISITION: &str = "http://opds-spec.org/acquisition";
/// `rel` value for the full-size cover image link.
pub const REL_IMAGE: &str = "http://opds-spec.org/image";
/// `rel` value for the cover thumbnail link.
pub const REL_IMAGE_THUMBNAIL: &str = "http://opds-spec.org/image/thumbnail";
/// `rel` value for the `OpenSearch` descriptor link.
pub const REL_SEARCH: &str = "search";
/// `rel` value for the next-page link (RFC 5005 — both feed kinds).
pub const REL_NEXT: &str = "next";
/// `rel` value for the self link on every feed.
pub const REL_SELF: &str = "self";
/// `rel` value for the start (root) link emitted on every feed.
pub const REL_START: &str = "start";
/// `rel` value reserved for parent-feed links on sub-feeds; not yet
/// emitted (MVP doesn't carry `rel="up"`).
pub const REL_UP: &str = "up";
/// `rel` value for navigation entries that point at a subcatalog.
pub const REL_SUBSECTION: &str = "subsection";

/// MIME type for the EPUB acquisition link's `type` attribute.
pub const EPUB_MIME: &str = "application/epub+zip";
/// MIME type for the `OpenSearch` descriptor link's `type` attribute.
pub const OPENSEARCH_DESCRIPTION_MIME: &str = "application/opensearchdescription+xml";

/// Discriminates the two OPDS 1.2 feed profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedKind {
    /// Navigation feed: links between subcatalogs (root, library, shelves).
    Navigation,
    /// Acquisition feed: actual book entries with download / cover links.
    Acquisition,
}

impl FeedKind {
    /// Profile-tagged `Content-Type` value for this feed kind.
    #[must_use]
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Navigation => NAVIGATION_TYPE,
            Self::Acquisition => ACQUISITION_TYPE,
        }
    }
}

/// One book row, ready for emission as an acquisition `<entry>`.
#[derive(Debug, Clone)]
pub struct AcquisitionEntry {
    /// Manifestation id; embedded in entry id and acquisition / cover URLs.
    pub manifestation_id: Uuid,
    /// Work title rendered as `<title>`.
    pub work_title: String,
    /// One author name per `<author>` element.
    pub creators: Vec<String>,
    /// Optional `<summary>` body.
    pub description: Option<String>,
    /// Optional language tag rendered as `<dc:language>`.
    pub language: Option<String>,
    /// Tag names rendered as `<category term="…" label="…"/>` elements.
    pub tags: Vec<String>,
    /// ISBN-13 preferred; ISBN-10 fallback. `None` emits a `urn:uuid:` id.
    pub isbn: Option<String>,
    /// Entry `<updated>` timestamp (sourced from `manifestations.updated_at`).
    pub updated_at: DateTime<Utc>,
}

/// Streaming OPDS feed builder.
///
/// Holds an in-memory `quick-xml` writer and the feed-level base URL +
/// kind discriminator. Construct with [`FeedBuilder::new`], append
/// entries / links via the `add_*` methods, then call
/// [`FeedBuilder::finish`] to recover the serialised XML bytes.
pub struct FeedBuilder {
    writer: Writer<Cursor<Vec<u8>>>,
    base_url: Url,
    kind: FeedKind,
}

impl FeedBuilder {
    /// Start a feed. Writes XML declaration, `<feed>` open with namespaces,
    /// `<id>` from `self_path`, `<title>`, `<updated>`, feed-level `<author>`,
    /// and `self` / `start` links. Acquisition feeds get the acquisition
    /// Content-Type profile on their self link; navigation feeds the
    /// navigation profile.
    ///
    /// # Panics
    ///
    /// Panics if a write to the underlying `quick-xml` writer fails, which
    /// does not happen: the writer backs onto a `Vec<u8>` and has no I/O to
    /// fail.
    #[must_use]
    pub fn new(
        base_url: &Url,
        self_path: &str,
        kind: FeedKind,
        title: &str,
        updated: DateTime<Utc>,
    ) -> Self {
        let mut writer = Writer::new(Cursor::new(Vec::new()));
        writer
            .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
            .expect("write xml decl");

        let mut feed = BytesStart::new("feed");
        feed.push_attribute(("xmlns", ATOM_NS));
        feed.push_attribute(("xmlns:opds", OPDS_NS));
        feed.push_attribute(("xmlns:dc", DC_NS));
        feed.push_attribute(("xmlns:opensearch", OPENSEARCH_NS));
        writer
            .write_event(Event::Start(feed))
            .expect("write feed open");

        write_text_element(&mut writer, "id", &feed_urn(self_path));
        write_text_element(&mut writer, "title", &sanitise_xml_text(title));
        write_text_element(
            &mut writer,
            "updated",
            &updated.to_rfc3339_opts(SecondsFormat::AutoSi, true),
        );

        // Feed-level <author>. RFC 4287 §4.1.1: a feed element MUST contain
        // one or more atom:author elements unless all of its atom:entry
        // children contain an atom:author element. Navigation entries don't
        // carry authors, so emit unconditionally.
        writer
            .write_event(Event::Start(BytesStart::new("author")))
            .expect("write author open");
        write_text_element(&mut writer, "name", "Reverie");
        writer
            .write_event(Event::End(BytesEnd::new("author")))
            .expect("write author close");

        let mut this = Self {
            writer,
            base_url: base_url.clone(),
            kind,
        };
        // Self link — carries the profile+kind so clients can distinguish
        // navigation and acquisition feeds at this URL.
        this.write_link(REL_SELF, self_path, Some(kind.content_type()), None);
        this.write_link(REL_START, "/opds", Some(NAVIGATION_TYPE), None);
        this
    }

    /// Absolute URL for a path relative to `base_url`. Falls back to the raw
    /// input if the join fails (should never happen with well-formed paths).
    fn abs(&self, path: &str) -> String {
        self.base_url
            .join(path)
            .map_or_else(|_| path.to_string(), |u| u.to_string())
    }

    fn write_link(&mut self, rel: &str, path: &str, mime: Option<&str>, title: Option<&str>) {
        let href = self.abs(path);
        let mut link = BytesStart::new("link");
        link.push_attribute(("rel", rel));
        link.push_attribute(("href", href.as_str()));
        if let Some(m) = mime {
            link.push_attribute(("type", m));
        }
        if let Some(t) = title {
            let sanitised = sanitise_xml_text(t);
            link.push_attribute(("title", sanitised.as_str()));
        }
        self.writer
            .write_event(Event::Empty(link))
            .expect("write link");
    }

    /// Add a navigation-feed entry pointing at a subsection (`rel="subsection"`
    /// when `rel_subsection`, else no rel — used for shelf entries on the root
    /// feed where the spec allows an omitted rel).
    ///
    /// # Panics
    ///
    /// Panics if a write to the underlying `quick-xml` writer fails, which
    /// does not happen: the writer backs onto a `Vec<u8>` and has no I/O to
    /// fail.
    pub fn add_navigation_entry(
        &mut self,
        id: &str,
        title: &str,
        href: &str,
        rel_subsection: bool,
    ) {
        self.writer
            .write_event(Event::Start(BytesStart::new("entry")))
            .expect("entry open");

        write_text_element(&mut self.writer, "id", id);
        write_text_element(&mut self.writer, "title", &sanitise_xml_text(title));
        write_text_element(
            &mut self.writer,
            "updated",
            &Utc::now().to_rfc3339_opts(SecondsFormat::AutoSi, true),
        );

        let abs_href = self.abs(href);
        let mut link = BytesStart::new("link");
        if rel_subsection {
            link.push_attribute(("rel", REL_SUBSECTION));
        }
        link.push_attribute(("href", abs_href.as_str()));
        link.push_attribute(("type", NAVIGATION_TYPE));
        self.writer
            .write_event(Event::Empty(link))
            .expect("nav link");

        self.writer
            .write_event(Event::End(BytesEnd::new("entry")))
            .expect("entry close");
    }

    /// Append one acquisition `<entry>`. Emits id (manifestation URN or
    /// ISBN URN), title, updated, authors, identifier, language,
    /// summary, categories, and the three OPDS rel links (acquisition /
    /// image / thumbnail). Every text field passes through
    /// [`super::xml::sanitise_xml_text`] before reaching `quick-xml`.
    ///
    /// # Panics
    ///
    /// Panics if a write to the underlying `quick-xml` writer fails, which
    /// does not happen: the writer backs onto a `Vec<u8>` and has no I/O to
    /// fail.
    pub fn add_acquisition_entry(&mut self, entry: &AcquisitionEntry) {
        self.writer
            .write_event(Event::Start(BytesStart::new("entry")))
            .expect("entry open");

        write_text_element(
            &mut self.writer,
            "id",
            &format!("urn:reverie:manifestation:{}", entry.manifestation_id),
        );
        write_text_element(
            &mut self.writer,
            "title",
            &sanitise_xml_text(&entry.work_title),
        );
        write_text_element(
            &mut self.writer,
            "updated",
            &entry
                .updated_at
                .to_rfc3339_opts(SecondsFormat::AutoSi, true),
        );

        for creator in &entry.creators {
            self.writer
                .write_event(Event::Start(BytesStart::new("author")))
                .expect("author open");
            write_text_element(&mut self.writer, "name", &sanitise_xml_text(creator));
            self.writer
                .write_event(Event::End(BytesEnd::new("author")))
                .expect("author close");
        }

        let identifier = entry.isbn.as_ref().map_or_else(
            || format!("urn:uuid:{}", entry.manifestation_id),
            |isbn| format!("urn:isbn:{}", sanitise_xml_text(isbn)),
        );
        write_text_element(&mut self.writer, "dc:identifier", &identifier);

        if let Some(lang) = &entry.language {
            write_text_element(&mut self.writer, "dc:language", &sanitise_xml_text(lang));
        }

        if let Some(desc) = &entry.description {
            let mut summary = BytesStart::new("summary");
            summary.push_attribute(("type", "text"));
            self.writer
                .write_event(Event::Start(summary))
                .expect("summary open");
            self.writer
                .write_event(Event::Text(BytesText::new(&sanitise_xml_text(desc))))
                .expect("summary text");
            self.writer
                .write_event(Event::End(BytesEnd::new("summary")))
                .expect("summary close");
        }

        for tag in &entry.tags {
            let sanitised = sanitise_xml_text(tag);
            let mut cat = BytesStart::new("category");
            cat.push_attribute(("term", sanitised.as_str()));
            cat.push_attribute(("label", sanitised.as_str()));
            self.writer
                .write_event(Event::Empty(cat))
                .expect("category");
        }

        // OPDS rel links — acquisition, image, thumbnail. Covers are emitted
        // under /opds/* so credentials stay in the paired RFC 7617 protection
        // space. The feed-level `type` attribute on the image link is
        // advisory — `image/jpeg` is a defensible default; clients re-check
        // on fetch.
        let id = entry.manifestation_id;
        self.write_link(
            REL_ACQUISITION,
            &format!("/opds/books/{id}/file"),
            Some(EPUB_MIME),
            None,
        );
        self.write_link(
            REL_IMAGE,
            &format!("/opds/books/{id}/cover"),
            Some("image/jpeg"),
            None,
        );
        self.write_link(
            REL_IMAGE_THUMBNAIL,
            &format!("/opds/books/{id}/cover/thumb"),
            Some("image/jpeg"),
            None,
        );

        self.writer
            .write_event(Event::End(BytesEnd::new("entry")))
            .expect("entry close");
    }

    /// `rel="next"` pagination link (RFC 5005 Atom paging — valid on
    /// both feed kinds; navigation feeds gained pagination as well).
    /// The link's `type` attribute follows the feed's own kind. Caller
    /// guarantees one page of rows is ready.
    pub fn add_next_link(&mut self, href: &str) {
        let mime = self.kind.content_type();
        self.write_link(REL_NEXT, href, Some(mime), None);
    }

    /// `rel="search"` pointing at the `OpenSearch` descriptor.
    pub fn add_search_link(&mut self, opensearch_xml_href: &str) {
        self.write_link(
            REL_SEARCH,
            opensearch_xml_href,
            Some(OPENSEARCH_DESCRIPTION_MIME),
            None,
        );
    }

    /// Arbitrary link — used e.g. for `rel="up"` on library/shelf roots.
    pub fn add_link(&mut self, rel: &str, path: &str, mime: Option<&str>) {
        self.write_link(rel, path, mime, None);
    }

    /// Close the `<feed>` element and return the serialised XML bytes.
    ///
    /// # Panics
    ///
    /// Panics if the write closing the `<feed>` element fails, which does
    /// not happen: the writer backs onto a `Vec<u8>` and has no I/O to fail.
    #[must_use]
    pub fn finish(mut self) -> Vec<u8> {
        self.writer
            .write_event(Event::End(BytesEnd::new("feed")))
            .expect("close feed");
        self.writer.into_inner().into_inner()
    }
}

fn write_text_element(writer: &mut Writer<Cursor<Vec<u8>>>, name: &str, text: &str) {
    writer
        .write_event(Event::Start(BytesStart::new(name)))
        .expect("text element open");
    writer
        .write_event(Event::Text(BytesText::new(text)))
        .expect("text element content");
    writer
        .write_event(Event::End(BytesEnd::new(name)))
        .expect("text element close");
}

/// Stable `urn:reverie:feed:<path>` id derived from the feed's self-path.
#[must_use]
pub fn feed_urn(self_path: &str) -> String {
    format!("urn:reverie:feed:{self_path}")
}

/// Stable `urn:reverie:author:<uuid>` id for an author navigation entry.
#[must_use]
pub fn author_urn(author_id: Uuid) -> String {
    format!("urn:reverie:author:{author_id}")
}

/// Stable `urn:reverie:series:<uuid>` id for a series navigation entry.
#[must_use]
pub fn series_urn(series_id: Uuid) -> String {
    format!("urn:reverie:series:{series_id}")
}

/// Stable `urn:reverie:shelf:<uuid>` id for a shelf navigation entry.
#[must_use]
pub fn shelf_urn(shelf_id: Uuid) -> String {
    format!("urn:reverie:shelf:{shelf_id}")
}

//! Publisher CommonMark to HTML that is safe to put on this site.
//!
//! Release notes are written by strangers and the protocol says so: "a consumer MUST treat
//! embedded HTML as untrusted". They are rendered here and then cleaned with an allowlist. What
//! survives is prose: paragraphs, emphasis, code, lists, quotes, links. Images do not survive,
//! because an image in a release note is a request from every reader's browser to a host the
//! publisher picked, which is exactly the beacon this site exists to avoid. Scripts, styles,
//! frames, forms and event handlers never had a chance.

use std::{collections::HashSet, sync::LazyLock};

use pulldown_cmark::{Options, Parser, html};

static CLEANER: LazyLock<ammonia::Builder<'static>> = LazyLock::new(|| {
    let mut builder = ammonia::Builder::empty();
    builder
        .tags(HashSet::from([
            "p",
            "br",
            "em",
            "strong",
            "del",
            "code",
            "pre",
            "ul",
            "ol",
            "li",
            "blockquote",
            "a",
            "h4",
            "h5",
            "h6",
            "hr",
            "table",
            "thead",
            "tbody",
            "tr",
            "th",
            "td",
        ]))
        .tag_attributes(std::collections::HashMap::from([(
            "a",
            HashSet::from(["href"]),
        )]))
        .url_schemes(HashSet::from(["http", "https", "mailto"]))
        // A relative link in a publisher's note points into the publisher's own site (or, like
        // Zola's `@/page.md`, into its sources), never into this one: the text stays, the target
        // goes.
        .url_relative(ammonia::UrlRelative::Deny)
        .link_rel(Some("nofollow noopener noreferrer"))
        .clean_content_tags(HashSet::from([
            "script", "style", "iframe", "object", "noscript", "template",
        ]));
    builder
});

fn render(text: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    let mut rendered = String::new();
    html::push_html(&mut rendered, Parser::new_ext(text, options));
    CLEANER.clean(&rendered).to_string()
}

/// Block CommonMark: `highlights`, `migration`, `notes`.
pub fn block(text: &str) -> String {
    render(text)
}

/// One list item or one line: the same rendering without the paragraph around it.
pub fn inline(text: &str) -> String {
    let rendered = render(text);
    let trimmed = rendered.trim();
    match trimmed
        .strip_prefix("<p>")
        .and_then(|inner| inner.strip_suffix("</p>"))
    {
        Some(inner) if !inner.contains("<p>") => inner.to_owned(),
        _ => trimmed.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_survives() {
        assert_eq!(
            inline("Install **Tallow** first; see `candlelight.cfg`."),
            "Install <strong>Tallow</strong> first; see <code>candlelight.cfg</code>."
        );
        assert!(block("- one\n- two\n").contains("<li>one</li>"));
    }

    #[test]
    fn scripts_handlers_and_frames_do_not() {
        for hostile in [
            "<script>alert(1)</script>",
            "<img src=x onerror=alert(1)>",
            "<iframe src=\"https://example.org\"></iframe>",
            "<a href=\"javascript:alert(1)\">click</a>",
            "[click](javascript:alert(1))",
            "<p style=\"background:url(https://tracker.example/)\">styled</p>",
            "<svg onload=alert(1)>",
        ] {
            let cleaned = block(hostile);
            let lowered = cleaned.to_lowercase();
            for forbidden in [
                "<script",
                "onerror",
                "<iframe",
                "javascript:",
                "style=",
                "<svg",
                "onload",
                "alert(1)</script",
            ] {
                assert!(
                    !lowered.contains(forbidden),
                    "{hostile:?} became {cleaned:?}"
                );
            }
        }
    }

    #[test]
    fn images_become_nothing_and_links_are_marked() {
        let cleaned =
            block("![tracker](https://tracker.example/pixel.gif) [site](https://example.org)");
        assert!(!cleaned.contains("<img"), "{cleaned}");
        assert!(!cleaned.contains("tracker.example"), "{cleaned}");
        assert!(
            cleaned.contains("rel=\"nofollow noopener noreferrer\""),
            "{cleaned}"
        );
    }

    #[test]
    fn relative_links_keep_their_text_and_lose_their_target() {
        let html = inline(
            "See [the Luau hosts](@/docs/luau-hosts.md), [the guide](guide/) and [crates.io](https://crates.io/).",
        );
        assert!(!html.contains("@/docs"), "{html}");
        assert!(!html.contains("href=\"guide/\""), "{html}");
        assert!(html.contains("the Luau hosts"), "{html}");
        assert!(html.contains("href=\"https://crates.io/\""), "{html}");
    }

    #[test]
    fn page_level_headings_are_flattened() {
        let cleaned = block("# Title\n\nBody");
        assert!(!cleaned.contains("<h1>"));
        assert!(cleaned.contains("Title"));
    }
}

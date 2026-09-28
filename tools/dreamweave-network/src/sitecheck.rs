//! `check-site`: every local link and anchor in the built site resolves, and no page repeats an
//! id. External links are not fetched; they belong to publishers, and a publisher's site being
//! down is network state, not a broken build.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use scraper::{Html, Selector};
use url::Url;

pub struct SiteReport {
    pub pages: usize,
    pub links: usize,
    pub problems: Vec<String>,
}

fn html_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory).with_context(|| format!("read {}", directory.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            html_files(&path, files)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "html")
        {
            files.push(path);
        }
    }
    Ok(())
}

/// The URL a file is served at, below `base`.
fn page_url(base: &Url, public: &Path, file: &Path) -> Result<Url> {
    let relative = file
        .strip_prefix(public)?
        .to_string_lossy()
        .replace('\\', "/");
    let relative = relative
        .strip_suffix("index.html")
        .unwrap_or(&relative)
        .to_owned();
    Ok(base.join(&relative)?)
}

/// Where a URL points, as far as the built site is concerned.
enum Target {
    /// Not under the site's base URL: an outbound link, not checked.
    Elsewhere,
    Missing,
    /// A directory named without its trailing slash: it works, through a redirect every host
    /// handles differently, and scripts comparing paths get it wrong.
    Unslashed,
    File(PathBuf),
}

fn target_file(base: &Url, public: &Path, url: &Url) -> Target {
    if url.origin() != base.origin() || !url.path().starts_with(base.path()) {
        return Target::Elsewhere;
    }
    let relative = percent_decode(&url.path()[base.path().len()..]);
    let candidate = public.join(&relative);
    let found = if relative.is_empty() || relative.ends_with('/') {
        candidate.join("index.html")
    } else if candidate.is_file() {
        candidate
    } else if candidate.join("index.html").is_file() {
        return Target::Unslashed;
    } else {
        return Target::Missing;
    };
    if found.is_file() {
        Target::File(found)
    } else {
        Target::Missing
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut position = 0;
    while position < bytes.len() {
        if bytes[position] == b'%'
            && let Some(byte) = text
                .get(position + 1..position + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            decoded.push(byte);
            position += 3;
            continue;
        }
        decoded.push(bytes[position]);
        position += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

pub fn check(public: &Path, base_url: &str) -> Result<SiteReport> {
    let base = Url::parse(&format!("{}/", base_url.trim_end_matches('/')))
        .with_context(|| format!("base URL {base_url:?}"))?;
    let mut files = Vec::new();
    html_files(public, &mut files)?;
    files.sort();

    let selector = Selector::parse("[href], [src]").expect("a static selector parses");
    let id_selector = Selector::parse("[id]").expect("a static selector parses");
    let mut ids: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
    let mut links: Vec<(PathBuf, Url, String)> = Vec::new();
    let mut problems = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).with_context(|| format!("read {}", file.display()))?;
        let document = Html::parse_document(&text);
        let mut seen = BTreeSet::new();
        for element in document.select(&id_selector) {
            let id = element.value().attr("id").unwrap_or_default().to_owned();
            if !seen.insert(id.clone()) {
                problems.push(format!("{}: id {id:?} appears twice", file.display()));
            }
        }
        ids.insert(file.clone(), seen);
        let here = page_url(&base, public, file)?;
        for element in document.select(&selector) {
            let value = element.value();
            for attribute in ["href", "src"] {
                let Some(reference) = value.attr(attribute) else {
                    continue;
                };
                if reference.starts_with("mailto:")
                    || reference.starts_with("data:")
                    || reference.is_empty()
                {
                    continue;
                }
                match here.join(reference) {
                    Ok(url) => links.push((file.clone(), url, reference.to_owned())),
                    Err(error) => problems.push(format!(
                        "{}: {reference:?} is not a URL: {error}",
                        file.display()
                    )),
                }
            }
        }
    }

    let mut checked = 0;
    for (file, url, reference) in &links {
        if !matches!(url.scheme(), "http" | "https") {
            problems.push(format!(
                "{}: {reference:?} uses scheme {}",
                file.display(),
                url.scheme()
            ));
            continue;
        }
        let target = match target_file(&base, public, url) {
            Target::Elsewhere => continue,
            Target::Missing => {
                checked += 1;
                problems.push(format!(
                    "{}: {reference} does not exist in the built site",
                    file.display()
                ));
                continue;
            }
            Target::Unslashed => {
                checked += 1;
                problems.push(format!(
                    "{}: {reference} names a directory without its trailing slash",
                    file.display()
                ));
                continue;
            }
            Target::File(target) => target,
        };
        checked += 1;
        if let Some(fragment) = url.fragment().filter(|fragment| !fragment.is_empty()) {
            let fragment = percent_decode(fragment);
            let known = ids.get(&target).is_some_and(|ids| ids.contains(&fragment));
            if !known
                && target
                    .extension()
                    .is_some_and(|extension| extension == "html")
            {
                problems.push(format!(
                    "{}: {reference} points at #{fragment}, which {} does not have",
                    file.display(),
                    target.display()
                ));
            }
        }
    }
    Ok(SiteReport {
        pages: files.len(),
        links: checked,
        problems,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_broken_links_missing_anchors_and_unslashed_directories() {
        let public = std::env::temp_dir().join(format!(
            "dreamweave-network-sitecheck-{}",
            std::process::id()
        ));
        fs::create_dir_all(public.join("projects")).unwrap();
        fs::write(
            public.join("projects/index.html"),
            r#"<h1 id="top">Projects</h1><a href="https://example.org/mods/">outbound</a>"#,
        )
        .unwrap();
        fs::write(
            public.join("index.html"),
            r#"<a href="https://example.org/mods/projects/">fine</a>
               <a href="https://example.org/mods/projects/#top">fine too</a>
               <a href="https://example.org/mods/projects">unslashed</a>
               <a href="https://example.org/mods/nowhere/">missing</a>
               <a href="https://example.org/mods/projects/#bottom">no such anchor</a>
               <a href="https://elsewhere.example.org/">not ours</a>
               <p id="twice"></p><p id="twice"></p>"#,
        )
        .unwrap();
        let report = check(&public, "https://example.org/mods").unwrap();
        fs::remove_dir_all(&public).unwrap();
        let problems = report.problems.join("\n");
        assert_eq!(report.problems.len(), 4, "{problems}");
        for expected in [
            "trailing slash",
            "does not exist",
            "#bottom",
            "appears twice",
        ] {
            assert!(problems.contains(expected), "{expected}: {problems}");
        }
    }
}

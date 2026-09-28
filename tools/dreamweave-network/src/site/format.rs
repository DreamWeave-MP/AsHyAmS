//! Small, boring text: labels, sizes, times, digests. Kept in one place so every page says the
//! same thing the same way.

use url::Url;

use crate::state::OriginRecord;

/// `2026-09-28T18:00:00Z` as `2026-09-28 18:00 UTC`. Observation times only; publisher dates
/// are dates and are shown as the publisher wrote them.
pub fn time_label(time: &str) -> String {
    match (time.get(..10), time.get(11..16)) {
        (Some(date), Some(clock)) if time.ends_with('Z') => format!("{date} {clock} UTC"),
        _ => time.to_owned(),
    }
}

pub fn date_of(time: &str) -> String {
    time.get(..10).unwrap_or(time).to_owned()
}

#[allow(clippy::cast_precision_loss)]
pub fn size_label(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} bytes");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// `4d8d9768277e…7a2ee9921`: enough to recognise, never enough to verify. The full digest is
/// always one click away.
pub fn short_digest(digest: &str) -> String {
    if digest.len() <= 24 {
        return digest.to_owned();
    }
    format!("{}…{}", &digest[..12], &digest[digest.len() - 8..])
}

pub fn runtime_label(runtime: &str) -> String {
    match runtime {
        "openmw" => "OpenMW".to_owned(),
        "mwse" => "MWSE".to_owned(),
        "tes3mp" => "TES3MP".to_owned(),
        "morrowind" => "Morrowind.exe".to_owned(),
        other => other.to_owned(),
    }
}

pub fn constraint_label(constraint: &str) -> String {
    if constraint.trim() == "*" {
        "any version".to_owned()
    } else {
        constraint.to_owned()
    }
}

pub fn platform_label(os: &str, arch: &str) -> String {
    let os = match os {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    };
    format!("{os} {arch}")
}

pub fn format_label(format: &str) -> String {
    match format {
        "flat" => "single directory".to_owned(),
        "bain" => "BAIN".to_owned(),
        "fomod" => "FOMOD + BAIN".to_owned(),
        "binary" => "program".to_owned(),
        other => other.to_owned(),
    }
}

pub fn link_label(name: &str) -> String {
    match name {
        "page" => "Project page".to_owned(),
        "source" => "Source".to_owned(),
        "issues" => "Issues".to_owned(),
        "documentation" => "Documentation".to_owned(),
        "support" => "Support".to_owned(),
        "donate" => "Support the author".to_owned(),
        "homepage" => "Homepage".to_owned(),
        "nexusmods" => "Nexus Mods".to_owned(),
        other => {
            let mut label = other.replace(['_', '-'], " ");
            if let Some(first) = label.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            label
        }
    }
}

/// What a person calls a site: the name its index gives, else its host and path.
pub fn origin_label(origin: &OriginRecord) -> String {
    if let Some(name) = &origin.site_name {
        return name.clone();
    }
    site_address(&origin.index_url)
}

/// `dreamweave-mp.github.io/DreamWeave-Mod-Template/` from the index URL.
pub fn site_address(index_url: &str) -> String {
    Url::parse(index_url)
        .ok()
        .and_then(|url| url.join("./").ok())
        .map_or_else(
            || index_url.to_owned(),
            |url| format!("{}{}", url.host_str().unwrap_or_default(), url.path()),
        )
}

/// A path segment made from a tag or other free text: lowercase ASCII letters, digits and
/// hyphens. Tags that slug to nothing get a digest instead, so no page is ever named "".
pub fn slug(text: &str) -> String {
    let mut slug = String::new();
    for character in text.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-').to_owned();
    if slug.is_empty() {
        crate::state::sha256_hex(text.as_bytes())[..12].to_owned()
    } else {
        slug
    }
}

pub fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(time_label("2026-09-28T18:05:09Z"), "2026-09-28 18:05 UTC");
        assert_eq!(size_label(252_892), "247.0 KiB");
        assert_eq!(size_label(512), "512 bytes");
        assert_eq!(
            short_digest("4d8d9768277e634a49a49d402dc65c221d50837368104724b5c202b027e9ce38"),
            "4d8d9768277e…27e9ce38"
        );
        assert_eq!(slug("OpenMW-Lua"), "openmw-lua");
        assert_eq!(slug("Lighting & Weather"), "lighting-weather");
        assert_eq!(slug("✨").len(), 12);
        assert_eq!(
            site_address("https://dreamweave-mp.github.io/DreamWeave-Mod-Template/dreamweave.json"),
            "dreamweave-mp.github.io/DreamWeave-Mod-Template/"
        );
    }
}

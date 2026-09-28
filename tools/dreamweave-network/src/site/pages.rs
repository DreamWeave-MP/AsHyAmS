//! One function per page family. Each writes its view data, then the content page that renders
//! it.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    path::Path,
};

use anyhow::{Context, Result};
use serde::Serialize;

use super::{
    Front, Writer,
    format::{origin_label, plural, short_digest, site_address, slug, time_label},
    views::{
        self, Card, EventRow, Fact, Link, NotesView, RelationshipGroup, ReleaseRow, ReleaseView,
        ReverseItem, SectionView, Stat, TagLink, card, event_row,
    },
};
use crate::{
    catalog::{self, IndexInfo},
    events::Tag,
    graph::{Graph, Node, NodeKind},
    network::{self, Claim, GapKey, Network, Resolution, capability_slug, claim_path},
    protocol::{DEVELOPMENT_CHANNEL, ProjectStatus, ProjectType, RelationshipKind, ReleaseStatus},
    state::{ClaimHealth, ClaimKey, OriginHealth},
};

pub(super) fn write_all(
    writer: &mut Writer,
    network: &Network,
    index: &IndexInfo,
    pages: &Path,
) -> Result<()> {
    site(writer, network, index)?;
    home(writer, network)?;
    projects(writer, network)?;
    for claim in network.claims.values() {
        if claim.manifest.is_some() {
            claim_page(writer, network, claim)?;
        }
    }
    identities(writer, network)?;
    updates(writer, network)?;
    releases(writer, network)?;
    dependencies(writer, network)?;
    capabilities(writer, network)?;
    gaps(writer, network)?;
    health(writer, network)?;
    origins(writer, network)?;
    conflicts(writer, network)?;
    browse(writer, network)?;
    public_data(writer, network, index, pages)?;
    Ok(())
}

// Shared --------------------------------------------------------------------------------------

#[derive(Serialize)]
struct SiteView {
    name: String,
    repository: String,
    observed_at: Option<String>,
    observed_label: Option<String>,
    crawler: Option<String>,
    conflicts: usize,
    stale: usize,
    unavailable: usize,
}

fn site(writer: &Writer, network: &Network, index: &IndexInfo) -> Result<()> {
    let view = SiteView {
        name: index.name.clone(),
        repository: index.repository.clone(),
        observed_at: network.observed_at.clone(),
        observed_label: network.observed_at.as_deref().map(time_label),
        crawler: network.crawler.clone(),
        conflicts: network.conflicts.len(),
        stale: network
            .listed()
            .iter()
            .filter(|claim| claim.record.health != ClaimHealth::Current)
            .count(),
        unavailable: network
            .origins
            .values()
            .filter(|origin| origin.health != OriginHealth::Healthy)
            .count(),
    };
    writer.view("site", &view)?;
    Ok(())
}

fn cards(network: &Network, keep: impl Fn(&Claim) -> bool) -> Vec<Card> {
    network
        .listed()
        .into_iter()
        .filter(|claim| keep(claim))
        .map(|claim| card(network, claim))
        .collect()
}

fn project_type(claim: &Claim) -> Option<ProjectType> {
    claim
        .manifest
        .as_ref()
        .map(|manifest| manifest.project.project_type)
}

fn project_status(claim: &Claim) -> Option<ProjectStatus> {
    claim
        .manifest
        .as_ref()
        .map(|manifest| manifest.project.status)
}

fn is_foundation(claim: &Claim) -> bool {
    matches!(
        project_type(claim),
        Some(ProjectType::Library | ProjectType::Framework)
    )
}

// Home ----------------------------------------------------------------------------------------

#[derive(Serialize)]
struct Featured {
    card: Card,
    note: String,
}

#[derive(Serialize)]
struct CardGroup {
    title: String,
    description: String,
    path: String,
    count: usize,
    cards: Vec<Card>,
}

#[derive(Serialize)]
struct BrowseLink {
    label: String,
    path: String,
    count: usize,
}

#[derive(Serialize)]
struct HomeView {
    stats: Vec<Stat>,
    featured: Vec<Featured>,
    latest_releases: Vec<ReleaseRow>,
    recent_events: Vec<EventRow>,
    groups: Vec<CardGroup>,
    problems: Vec<Link>,
    browse: Vec<BrowseLink>,
    healthy_origins: usize,
    origins: usize,
    current_claims: usize,
    claims: usize,
}

fn group(title: &str, description: &str, path: &str, all: Vec<Card>) -> Option<CardGroup> {
    (!all.is_empty()).then(|| CardGroup {
        title: title.to_owned(),
        description: description.to_owned(),
        path: path.to_owned(),
        count: all.len(),
        cards: all.into_iter().take(6).collect(),
    })
}

fn home_groups(network: &Network) -> Vec<CardGroup> {
    [
        group(
            "Mods",
            "Active mods, by name.",
            "browse/type-mod/",
            cards(network, |claim| {
                project_type(claim) == Some(ProjectType::Mod)
                    && project_status(claim) == Some(ProjectStatus::Active)
            }),
        ),
        group(
            "Libraries and frameworks",
            "What other projects build on.",
            "browse/foundations/",
            cards(network, is_foundation),
        ),
        group(
            "Tools",
            "Programs and utilities.",
            "browse/type-tool/",
            cards(network, |claim| {
                project_type(claim) == Some(ProjectType::Tool)
            }),
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn home(writer: &mut Writer, network: &Network) -> Result<()> {
    let featured = network
        .curation
        .featured
        .iter()
        .filter_map(|item| {
            let key = network.claims_of(&item.project).into_iter().next()?;
            Some(Featured {
                card: card(network, &network.claims[&key]),
                note: item.note.clone(),
            })
        })
        .collect();
    let groups = home_groups(network);
    let problems = network
        .origins
        .values()
        .filter(|origin| origin.health != OriginHealth::Healthy)
        .map(|origin| Link {
            label: format!("{}: {}", origin_label(origin), origin.health.label()),
            url: views::origin_path(&origin.id),
        })
        .chain(network.conflicts.iter().map(|conflict| Link {
            label: format!("identity conflict on {}", conflict.project),
            url: format!("projects/{}/", conflict.project),
        }))
        .take(5)
        .collect();
    let listed = network.listed();
    let view = HomeView {
        stats: views::network_stats(network),
        featured,
        latest_releases: views::release_rows(network)
            .into_iter()
            .filter(|row| row.channel != DEVELOPMENT_CHANNEL && row.status == "available")
            .take(8)
            .collect(),
        recent_events: network
            .events
            .iter()
            .map(|event| event_row(network, event))
            .filter(|row| !row.development_only)
            .take(10)
            .collect(),
        groups,
        problems,
        browse: browse_facets(network)
            .into_iter()
            .map(|facet| BrowseLink {
                label: facet.title,
                path: format!("browse/{}/", facet.slug),
                count: facet.cards.len(),
            })
            .filter(|link| link.count > 0)
            .collect(),
        healthy_origins: network
            .origins
            .values()
            .filter(|origin| origin.health == OriginHealth::Healthy)
            .count(),
        origins: network.origins.len(),
        current_claims: listed
            .iter()
            .filter(|claim| claim.record.health == ClaimHealth::Current)
            .count(),
        claims: listed.len(),
    };
    let path = writer.view("home", &view)?;
    writer.page(
        "_index.md",
        &Front {
            title: "DreamWeave Network",
            description: "A map of places DreamWeave mods live: every project, release, dependency and change this index has observed on independently published sites.",
            template: "home.html",
            view: Some(&path),
        },
    )
}

// The catalog and browse views ----------------------------------------------------------------

#[derive(Serialize)]
struct Facet {
    label: String,
    token: String,
    count: usize,
    path: String,
}

#[derive(Serialize)]
struct ProjectsView {
    cards: Vec<Card>,
    types: Vec<Facet>,
    statuses: Vec<Facet>,
    tags: Vec<Facet>,
}

fn projects(writer: &mut Writer, network: &Network) -> Result<()> {
    let all = cards(network, |_| true);
    let types = ProjectType::ALL
        .iter()
        .map(|kind| Facet {
            label: kind.plural().to_owned(),
            token: format!("type:{}", kind.token()),
            count: all
                .iter()
                .filter(|card| card.type_token == kind.token())
                .count(),
            path: format!("browse/type-{}/", kind.token()),
        })
        .filter(|facet| facet.count > 0)
        .collect();
    let statuses = ProjectStatus::ALL
        .iter()
        .map(|status| Facet {
            label: status.token().to_owned(),
            token: format!("status:{}", status.token()),
            count: all
                .iter()
                .filter(|card| card.status == status.token())
                .count(),
            path: format!("browse/status-{}/", status.token()),
        })
        .filter(|facet| facet.count > 0)
        .collect();
    let view = ProjectsView {
        types,
        statuses,
        tags: tag_counts(&all)
            .into_iter()
            .map(|(tag, count)| Facet {
                token: format!("tag:{}", slug(&tag.label)),
                label: tag.label,
                count,
                path: tag.path,
            })
            .collect(),
        cards: all,
    };
    let path = writer.view("projects", &view)?;
    writer.page(
        "projects/_index.md",
        &Front {
            title: "Projects",
            description: "Every project claim this index holds, one card per claim, with its publishing site.",
            template: "projects.html",
            view: Some(&path),
        },
    )
}

fn tag_counts(cards: &[Card]) -> Vec<(TagLink, usize)> {
    let mut counts: BTreeMap<String, (TagLink, usize)> = BTreeMap::new();
    for card in cards {
        for tag in &card.tags {
            counts
                .entry(slug(&tag.label))
                .or_insert_with(|| (tag.clone(), 0))
                .1 += 1;
        }
    }
    let mut counts: Vec<(TagLink, usize)> = counts.into_values().collect();
    counts.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.0.label.cmp(&right.0.label))
    });
    counts
}

#[derive(Serialize)]
struct BrowseView {
    slug: String,
    title: String,
    description: String,
    cards: Vec<Card>,
}

fn browse_facets(network: &Network) -> Vec<BrowseView> {
    let mut facets = Vec::new();
    for kind in ProjectType::ALL {
        facets.push(BrowseView {
            slug: format!("type-{}", kind.token()),
            title: kind.plural().to_owned(),
            description: format!("Projects whose manifest declares type {}.", kind.token()),
            cards: cards(network, |claim| project_type(claim) == Some(kind)),
        });
    }
    facets.push(BrowseView {
        slug: "foundations".to_owned(),
        title: "Libraries and frameworks".to_owned(),
        description: "Projects other projects build on, with what uses them.".to_owned(),
        cards: cards(network, is_foundation),
    });
    for status in ProjectStatus::ALL {
        facets.push(BrowseView {
            slug: format!("status-{}", status.token()),
            title: format!("Status: {}", status.token()),
            description: format!("Projects whose publisher says they are {}.", status.token()),
            cards: cards(network, |claim| project_status(claim) == Some(status)),
        });
    }
    let mut recent = cards(network, |_| true);
    recent.sort_by(|left, right| {
        right
            .updated
            .cmp(&left.updated)
            .then_with(|| left.name.cmp(&right.name))
    });
    facets.push(BrowseView {
        slug: "recently-released".to_owned(),
        title: "Recently released".to_owned(),
        description: "By the newest release date each publisher gives.".to_owned(),
        cards: recent,
    });
    let mut changed: Vec<(Option<String>, Card)> = network
        .listed()
        .into_iter()
        .map(|claim| (claim.record.last_changed.clone(), card(network, claim)))
        .collect();
    changed.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.name.cmp(&right.1.name))
    });
    facets.push(BrowseView {
        slug: "recently-updated".to_owned(),
        title: "Recently updated".to_owned(),
        description: "By when this index last saw each manifest change.".to_owned(),
        cards: changed.into_iter().map(|(_, card)| card).collect(),
    });
    let all = cards(network, |_| true);
    for (tag, _) in tag_counts(&all) {
        let wanted = slug(&tag.label);
        facets.push(BrowseView {
            slug: format!("tag-{wanted}"),
            title: format!("Tag: {}", tag.label),
            description: format!("Projects their publishers tagged {}. Tags are the publishers' words, not a taxonomy.", tag.label),
            cards: all
                .iter()
                .filter(|card| card.tags.iter().any(|item| slug(&item.label) == wanted))
                .cloned()
                .collect(),
        });
    }
    facets
}

fn browse(writer: &mut Writer, network: &Network) -> Result<()> {
    let facets = browse_facets(network);
    let links: Vec<BrowseLink> = facets
        .iter()
        .map(|facet| BrowseLink {
            label: facet.title.clone(),
            path: format!("browse/{}/", facet.slug),
            count: facet.cards.len(),
        })
        .collect();
    let path = writer.view("browse", &links)?;
    writer.page(
        "browse/_index.md",
        &Front {
            title: "Browse",
            description: "Projects by type, status, tag and recency.",
            template: "browse-index.html",
            view: Some(&path),
        },
    )?;
    for facet in facets {
        let path = writer.view(&format!("browse/{}", facet.slug), &facet)?;
        writer.page(
            &format!("browse/{}.md", facet.slug),
            &Front {
                title: &facet.title,
                description: &facet.description,
                template: "browse.html",
                view: Some(&path),
            },
        )?;
    }
    Ok(())
}

// A claim's record ----------------------------------------------------------------------------

#[derive(Serialize)]
struct OtherClaim {
    path: Option<String>,
    origin_label: String,
    name: String,
    health: String,
}

#[derive(Serialize)]
struct Person {
    name: String,
    role: Option<String>,
    url: Option<String>,
}

#[derive(Serialize)]
struct MediaItem {
    kind: String,
    url: String,
    alt: String,
    caption: Option<String>,
    category: Option<String>,
    featured: bool,
}

#[derive(Serialize)]
struct ComponentRow {
    id: String,
    name: String,
    description: Option<String>,
    required: bool,
    default: bool,
    group: Option<String>,
}

#[derive(Serialize)]
struct CapabilityLink {
    capability: String,
    path: String,
}

#[derive(Serialize)]
struct ClaimView {
    card: Card,
    links: Vec<Link>,
    record: Vec<Fact>,
    notice: Option<String>,
    problem: Option<String>,
    compatibility: Vec<Fact>,
    relationships: Vec<RelationshipGroup>,
    provides: Vec<CapabilityLink>,
    reverse: Vec<ReverseItem>,
    graph: Option<String>,
    releases: Vec<ReleaseView>,
    release_summary: String,
    components: Vec<ComponentRow>,
    media: Vec<MediaItem>,
    maintainers: Vec<Person>,
    credits: Vec<Person>,
    events: Vec<EventRow>,
    others: Vec<OtherClaim>,
    superseded_by: Option<Link>,
    withdrawn: bool,
    license: Option<String>,
    game: String,
    versioning: String,
    manifest_url: String,
}

fn cached_notice(network: &Network, claim: &Claim) -> Option<String> {
    if claim.record.health == ClaimHealth::Current {
        return None;
    }
    let success = claim
        .record
        .last_success
        .as_deref()
        .map_or_else(|| "never".to_owned(), time_label);
    Some(format!(
        "Cached from the last successful observation, {success}. As of {}, this claim is {}.",
        network
            .observed_at
            .as_deref()
            .map_or_else(|| time_label(&claim.record.last_attempt), time_label),
        claim.record.health.label()
    ))
}

fn record_facts(network: &Network, claim: &Claim) -> Vec<Fact> {
    let origin = network.origins.get(&claim.key.origin);
    let mut facts = vec![
        Fact::code("Project id", claim.key.project.to_string())
            .path(format!("projects/{}/", claim.key.project)),
        Fact::text(
            "Published by",
            origin.map_or_else(|| claim.key.origin.clone(), origin_label),
        )
        .path(views::origin_path(&claim.key.origin)),
    ];
    if let Some(origin) = origin {
        facts
            .push(Fact::code("Site index", origin.index_url.clone()).url(origin.index_url.clone()));
    }
    facts.push(
        Fact::code("Manifest", claim.record.entry.manifest.clone())
            .url(claim.record.entry.manifest.clone()),
    );
    if let Some(ingested) = &claim.record.ingested_sha256 {
        facts.push(Fact::code("Held manifest sha256", ingested.clone()));
    }
    if claim.record.ingested_sha256.as_ref() != Some(&claim.record.advertised_sha256) {
        facts.push(Fact::code(
            "Advertised sha256",
            claim.record.advertised_sha256.clone(),
        ));
    }
    facts.push(Fact::text("Claim health", claim.record.health.label()));
    facts.push(Fact::text(
        "First observed",
        time_label(&claim.record.first_observed),
    ));
    if let Some(changed) = &claim.record.last_changed {
        facts.push(Fact::text("Manifest last changed", time_label(changed)));
    }
    if let Some(success) = &claim.record.last_success {
        facts.push(Fact::text("Last confirmed current", time_label(success)));
    }
    facts.push(Fact::text(
        "Last attempt",
        time_label(&claim.record.last_attempt),
    ));
    facts
}

fn claim_compatibility(claim: &Claim) -> Vec<Fact> {
    let Some(release) = claim.current_release() else {
        return Vec::new();
    };
    let mut facts: Vec<Fact> = release
        .runtimes
        .iter()
        .map(|(runtime, constraint)| {
            Fact::code(
                &super::format::runtime_label(runtime),
                super::format::constraint_label(constraint),
            )
        })
        .collect();
    if let Some(openmw) = release.openmw() {
        if let Some(lua_api) = openmw.lua_api {
            facts.push(Fact::code("Lua API", lua_api));
        }
        if !openmw.requires_content.is_empty() {
            facts.push(Fact::code(
                "Requires content",
                openmw.requires_content.join(", "),
            ));
        }
        if !openmw.settings.is_empty() {
            facts.push(Fact::text(
                "Settings",
                plural(
                    openmw.settings.len(),
                    "setting the publisher asks for",
                    "settings the publisher asks for",
                ),
            ));
        }
    }
    facts.push(Fact::text(
        "Platforms",
        if release.platforms.is_empty() {
            "not restricted".to_owned()
        } else {
            release
                .platforms
                .iter()
                .map(|platform| super::format::platform_label(&platform.os, &platform.arch))
                .collect::<Vec<_>>()
                .join(", ")
        },
    ));
    if !release.critical_extensions.is_empty() {
        facts.push(Fact::code(
            "Critical extensions",
            release.critical_extensions.join(", "),
        ));
    }
    facts
}

fn neighborhood(network: &Network, claim: &Claim) -> Option<String> {
    let mut graph = Graph::default();
    let focus = claim.path();
    graph.add_node(Node {
        id: focus.clone(),
        label: claim.name().to_owned(),
        detail: claim
            .current
            .as_ref()
            .map(|current| current.version.clone())
            .unwrap_or_default(),
        href: None,
        kind: NodeKind::Focus,
    });
    let node = |key: &ClaimKey| {
        let target = &network.claims[key];
        Node {
            id: target.path(),
            label: target.name().to_owned(),
            detail: target
                .manifest
                .as_ref()
                .map(|manifest| manifest.project.project_type.token().to_owned())
                .unwrap_or_default(),
            href: Some(format!("../../../{}", target.path())),
            kind: if is_foundation(target) {
                NodeKind::Foundation
            } else {
                NodeKind::Project
            },
        }
    };
    for edge in network.forward_edges(&claim.key) {
        if edge.targets().is_empty() {
            let name = edge
                .relationship
                .name
                .clone()
                .or_else(|| edge.relationship.capability.clone())
                .unwrap_or_default();
            let id = format!("external:{name}");
            graph.add_node(Node {
                id: id.clone(),
                label: name,
                detail: "not indexed".to_owned(),
                href: None,
                kind: NodeKind::External,
            });
            graph.add_edge(&focus, &id, edge.kind().token());
        }
        for target in edge.targets() {
            graph.add_node(node(target));
            graph.add_edge(&focus, &network.claims[target].path(), edge.kind().token());
        }
    }
    for edge in network.reverse_edges(&claim.key) {
        graph.add_node(node(&edge.from));
        graph.add_edge(
            &network.claims[&edge.from].path(),
            &focus,
            edge.kind().token(),
        );
    }
    graph.render(
        &format!("{} and its direct relationships", claim.name()),
        "Dependents above, dependencies below. The tables on this page list the same relationships.",
    )
}

fn claim_links(project: &crate::protocol::Project) -> Vec<Link> {
    let mut links = Vec::new();
    for name in [
        "page",
        "documentation",
        "source",
        "issues",
        "support",
        "homepage",
        "donate",
    ] {
        if let Some(url) = project.links.get(name) {
            links.push(Link {
                label: super::format::link_label(name),
                url: url.clone(),
            });
        }
    }
    for (name, url) in &project.links {
        if !links.iter().any(|link| &link.url == url) {
            links.push(Link {
                label: super::format::link_label(name),
                url: url.clone(),
            });
        }
    }
    if let Some(nexus) = &project.integrations.nexusmods {
        links.push(Link {
            label: format!("Nexus Mods {}", nexus.mod_id),
            url: format!(
                "https://www.nexusmods.com/{}/mods/{}",
                nexus.game, nexus.mod_id
            ),
        });
    }
    links
}

fn claim_components(claim: &Claim) -> Vec<ComponentRow> {
    let Some(release) = claim.current_release() else {
        return Vec::new();
    };
    release
        .components
        .iter()
        .map(|component| ComponentRow {
            id: component.id.clone(),
            name: component.name.clone(),
            description: component.description.clone(),
            required: component.required,
            default: component.default,
            group: component.group.as_ref().map(|group| {
                release
                    .groups
                    .iter()
                    .find(|item| &item.id == group)
                    .map_or_else(
                        || group.clone(),
                        |item| format!("{} ({})", item.name, item.select),
                    )
            }),
        })
        .collect()
}

fn claim_media(project: &crate::protocol::Project) -> Vec<MediaItem> {
    project
        .media
        .iter()
        .map(|item| MediaItem {
            kind: match item.kind {
                crate::protocol::MediaKind::Image => "image",
                crate::protocol::MediaKind::Video => "video",
            }
            .to_owned(),
            url: item.url.clone(),
            alt: item.alt.clone(),
            caption: item.caption.clone(),
            category: item.category.clone(),
            featured: item.featured == Some(true),
        })
        .collect()
}

fn claim_people(project: &crate::protocol::Project) -> (Vec<Person>, Vec<Person>) {
    let maintainers = project
        .maintainers
        .iter()
        .map(|person| Person {
            name: person.name.clone(),
            role: None,
            url: person.url.clone(),
        })
        .collect();
    let credits = project
        .credits
        .iter()
        .map(|credit| Person {
            name: credit.name.clone(),
            role: credit.role.clone(),
            url: credit.url.clone(),
        })
        .collect();
    (maintainers, credits)
}

fn claim_others(network: &Network, claim: &Claim) -> Vec<OtherClaim> {
    network
        .claims
        .values()
        .filter(|other| other.key.project == claim.key.project && other.key != claim.key)
        .map(|other| OtherClaim {
            path: other.manifest.is_some().then(|| other.path()),
            origin_label: network
                .origins
                .get(&other.key.origin)
                .map_or_else(|| other.key.origin.clone(), origin_label),
            name: other.name().to_owned(),
            health: other.record.health.label().to_owned(),
        })
        .collect()
}

fn claim_view(network: &Network, claim: &Claim) -> ClaimView {
    let manifest = claim
        .manifest
        .as_ref()
        .expect("claim pages are made for claims with a manifest");
    let project = &manifest.project;
    let mut releases: Vec<ReleaseView> = manifest
        .releases
        .iter()
        .map(|release| views::release_view(manifest, release, claim.current.as_ref()))
        .collect();
    releases.sort_by(|left, right| network::newest_first(manifest, &left.version, &right.version));
    let forward = network.forward_edges(&claim.key);
    let (maintainers, credits) = claim_people(project);
    ClaimView {
        card: card(network, claim),
        links: claim_links(project),
        record: record_facts(network, claim),
        notice: cached_notice(network, claim),
        problem: claim.record.problem.clone(),
        compatibility: claim_compatibility(claim),
        relationships: views::relationship_groups(network, &forward),
        provides: claim
            .current_release()
            .map(|release| {
                release
                    .provides
                    .iter()
                    .map(|capability| CapabilityLink {
                        capability: capability.clone(),
                        path: format!("capabilities/{}/", capability_slug(capability)),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        reverse: views::reverse_items(network, &claim.key),
        graph: neighborhood(network, claim),
        release_summary: views::release_status_counts(manifest),
        releases,
        components: claim_components(claim),
        media: claim_media(project),
        maintainers,
        credits,
        events: network
            .events
            .iter()
            .filter(|event| event.project == claim.key.project && event.origin == claim.key.origin)
            .map(|event| event_row(network, event))
            .take(20)
            .collect(),
        others: claim_others(network, claim),
        superseded_by: claim.superseded_by.as_ref().map(|key| Link {
            label: network
                .origins
                .get(&key.origin)
                .map_or_else(|| key.origin.clone(), origin_label),
            url: claim_path(key),
        }),
        withdrawn: claim.record.health == ClaimHealth::Withdrawn,
        license: project.license.clone(),
        game: project.game.clone(),
        versioning: project.versioning.name().to_owned(),
        manifest_url: claim.record.entry.manifest.clone(),
    }
}

fn claim_page(writer: &mut Writer, network: &Network, claim: &Claim) -> Result<()> {
    let view = claim_view(network, claim);
    let path = writer.view(
        &format!("claims/{}/{}", claim.key.project, claim.key.origin),
        &view,
    )?;
    let description = view.card.summary.clone().unwrap_or_else(|| {
        format!(
            "{} as published by {}.",
            view.card.name, view.card.origin_label
        )
    });
    writer.page(
        &format!("projects/{}/{}.md", claim.key.project, claim.key.origin),
        &Front {
            title: &view.card.name,
            description: &description,
            template: "claim.html",
            view: Some(&path),
        },
    )
}

#[derive(Serialize)]
struct IdentityClaim {
    path: Option<String>,
    name: String,
    origin_label: String,
    origin_path: String,
    health: String,
    health_label: String,
    manifest_sha256: Option<String>,
    first_observed: String,
    last_success: Option<String>,
    superseded_by: Option<String>,
    signatures: usize,
}

#[derive(Serialize)]
struct IdentityView {
    project: String,
    conflict: bool,
    claims: Vec<IdentityClaim>,
}

fn identity_claims(network: &Network, project: &crate::protocol::ProjectId) -> Vec<IdentityClaim> {
    network
        .claims
        .values()
        .filter(|claim| &claim.key.project == project)
        .map(|claim| IdentityClaim {
            path: claim.manifest.is_some().then(|| claim.path()),
            name: claim.name().to_owned(),
            origin_label: network
                .origins
                .get(&claim.key.origin)
                .map_or_else(|| claim.key.origin.clone(), origin_label),
            origin_path: views::origin_path(&claim.key.origin),
            health: claim.record.health.token().to_owned(),
            health_label: claim.record.health.label().to_owned(),
            manifest_sha256: claim.record.ingested_sha256.clone(),
            first_observed: time_label(&claim.record.first_observed),
            last_success: claim.record.last_success.as_deref().map(time_label),
            superseded_by: claim.superseded_by.as_ref().map(claim_path),
            signatures: claim.manifest.as_ref().map_or(0, |manifest| {
                manifest
                    .releases
                    .iter()
                    .flat_map(|release| &release.artifacts)
                    .map(|artifact| artifact.signatures.len())
                    .sum()
            }),
        })
        .collect()
}

fn identities(writer: &mut Writer, network: &Network) -> Result<()> {
    let projects: BTreeSet<&crate::protocol::ProjectId> =
        network.claims.keys().map(|key| &key.project).collect();
    for project in projects {
        let view = IdentityView {
            project: project.to_string(),
            conflict: network
                .conflicts
                .iter()
                .any(|conflict| &conflict.project == project),
            claims: identity_claims(network, project),
        };
        let name = view
            .claims
            .first()
            .map_or_else(|| project.to_string(), |claim| claim.name.clone());
        let path = writer.view(&format!("identities/{project}"), &view)?;
        writer.page(
            &format!("projects/{project}/_index.md"),
            &Front {
                title: &name,
                description: &format!("Every claim this index holds for project id {project}."),
                template: "identity.html",
                view: Some(&path),
            },
        )?;
    }
    Ok(())
}

// Updates -------------------------------------------------------------------------------------

#[derive(Serialize)]
struct Filter {
    token: String,
    label: String,
    description: String,
    path: String,
    count: usize,
    active: bool,
}

#[derive(Serialize)]
struct UpdatesView {
    title: String,
    description: String,
    filters: Vec<Filter>,
    rows: Vec<EventRow>,
}

#[derive(Serialize)]
struct EventView {
    row: EventRow,
    sections: Vec<SectionView>,
    notes: Vec<NotesView>,
    before: Option<String>,
    after: Option<String>,
    before_short: Option<String>,
    after_short: Option<String>,
    moved_from: Option<String>,
    manifest_url: Option<String>,
}

fn updates(writer: &mut Writer, network: &Network) -> Result<()> {
    let rows: Vec<EventRow> = network
        .events
        .iter()
        .map(|event| event_row(network, event))
        .collect();
    let filters = |active: Option<Tag>| -> Vec<Filter> {
        Tag::ALL
            .iter()
            .map(|tag| Filter {
                token: tag.token().to_owned(),
                label: tag.label().to_owned(),
                description: tag.description().to_owned(),
                path: format!("updates/{}/", tag.token()),
                count: rows
                    .iter()
                    .filter(|row| row.tags.iter().any(|item| item == tag.token()))
                    .count(),
                active: active == Some(*tag),
            })
            .collect()
    };
    let view = UpdatesView {
        title: "Updates".to_owned(),
        description: "Every change this index has observed, newest first. Each entry is a typed difference between two manifests of one claim.".to_owned(),
        filters: filters(None),
        rows: rows.clone(),
    };
    let path = writer.view("updates", &view)?;
    writer.page(
        "updates/_index.md",
        &Front {
            title: "Updates",
            description: &view.description,
            template: "updates.html",
            view: Some(&path),
        },
    )?;
    for tag in Tag::ALL {
        let view = UpdatesView {
            title: format!("Updates: {}", tag.label()),
            description: tag.description().to_owned(),
            filters: filters(Some(tag)),
            rows: rows
                .iter()
                .filter(|row| row.tags.iter().any(|item| item == tag.token()))
                .cloned()
                .collect(),
        };
        let path = writer.view(&format!("updates/{}", tag.token()), &view)?;
        writer.page(
            &format!("updates/{}.md", tag.token()),
            &Front {
                title: &view.title,
                description: &view.description,
                template: "updates.html",
                view: Some(&path),
            },
        )?;
    }
    for event in &network.events {
        let key = ClaimKey {
            project: event.project.clone(),
            origin: event.origin.clone(),
        };
        let row = event_row(network, event);
        let view = EventView {
            sections: views::event_sections(network, event),
            notes: views::event_notes(event),
            before: event.before.clone(),
            after: event.after.clone(),
            before_short: event.before.as_deref().map(short_digest),
            after_short: event.after.as_deref().map(short_digest),
            moved_from: event.moved_from.clone(),
            manifest_url: network
                .claims
                .get(&key)
                .map(|claim| claim.record.entry.manifest.clone()),
            row,
        };
        let path = writer.view(&format!("events/{}", event.id), &view)?;
        writer.page(
            &format!("updates/{}.md", event.id),
            &Front {
                title: &format!("{}: {}", event.name, view.row.headline),
                description: &format!(
                    "{} at {}, observed {}.",
                    event.name, view.row.origin_label, view.row.observed_label
                ),
                template: "event.html",
                view: Some(&path),
            },
        )?;
    }
    Ok(())
}

// Releases ------------------------------------------------------------------------------------

#[derive(Serialize)]
struct ReleasesView {
    title: String,
    description: String,
    filters: Vec<Filter>,
    rows: Vec<ReleaseRow>,
}

/// A release filter page: token, label, description, and which rows it keeps.
type ReleaseFilter = (
    &'static str,
    &'static str,
    &'static str,
    fn(&ReleaseRow) -> bool,
);

fn releases(writer: &mut Writer, network: &Network) -> Result<()> {
    let rows = views::release_rows(network);
    let kinds: [ReleaseFilter; 4] = [
        (
            "stable",
            "Stable",
            "Releases in the stable channel.",
            |row| row.channel == "stable",
        ),
        (
            "development",
            "Development builds",
            "The rolling development channel.",
            |row| row.channel == DEVELOPMENT_CHANNEL,
        ),
        (
            "yanked",
            "Yanked",
            "Releases their publishers withdrew. They stay listed, as the protocol requires.",
            |row| row.status == ReleaseStatus::Yanked.token(),
        ),
        (
            "deprecated",
            "Deprecated",
            "Releases their publishers deprecated.",
            |row| row.status == ReleaseStatus::Deprecated.token(),
        ),
    ];
    let filters = |active: Option<&str>| -> Vec<Filter> {
        kinds
            .iter()
            .map(|(token, label, description, keep)| Filter {
                token: (*token).to_owned(),
                label: (*label).to_owned(),
                description: (*description).to_owned(),
                path: format!("releases/{token}/"),
                count: rows.iter().filter(|row| keep(row)).count(),
                active: active == Some(*token),
            })
            .collect()
    };
    let description = "Every release of every project claim, newest first by the date its publisher gives. The date this index first saw each release is shown separately and is never the same fact.";
    let view = ReleasesView {
        title: "Releases".to_owned(),
        description: description.to_owned(),
        filters: filters(None),
        rows: rows.clone(),
    };
    let path = writer.view("releases", &view)?;
    writer.page(
        "releases/_index.md",
        &Front {
            title: "Releases",
            description,
            template: "releases.html",
            view: Some(&path),
        },
    )?;
    for (token, label, description, keep) in kinds {
        let view = ReleasesView {
            title: format!("Releases: {label}"),
            description: description.to_owned(),
            filters: filters(Some(token)),
            rows: rows.iter().filter(|row| keep(row)).cloned().collect(),
        };
        let path = writer.view(&format!("releases/{token}"), &view)?;
        writer.page(
            &format!("releases/{token}.md"),
            &Front {
                title: &view.title,
                description,
                template: "releases.html",
                view: Some(&path),
            },
        )?;
    }
    Ok(())
}

// Dependencies --------------------------------------------------------------------------------

#[derive(Serialize)]
struct EdgeRow {
    from: String,
    from_path: String,
    kind: String,
    item: views::RelationshipItem,
}

#[derive(Serialize)]
struct UsedBy {
    name: String,
    path: String,
    count: usize,
    type_label: String,
}

#[derive(Serialize)]
struct DependenciesView {
    overview: Option<String>,
    foundations: Option<String>,
    rows: Vec<EdgeRow>,
    used_by: Vec<UsedBy>,
    unresolved: usize,
}

fn graph_node(network: &Network, key: &ClaimKey) -> Node {
    let claim = &network.claims[key];
    Node {
        id: claim.path(),
        label: claim.name().to_owned(),
        detail: claim
            .manifest
            .as_ref()
            .map(|manifest| manifest.project.project_type.token().to_owned())
            .unwrap_or_default(),
        href: Some(format!("../{}", claim.path())),
        kind: if is_foundation(claim) {
            NodeKind::Foundation
        } else {
            NodeKind::Project
        },
    }
}

fn ecosystem_graph(network: &Network, keep: impl Fn(&network::Edge) -> bool) -> Graph {
    let mut graph = Graph::default();
    for edge in &network.edges {
        if !keep(edge) {
            continue;
        }
        for target in edge.targets() {
            graph.add_node(graph_node(network, &edge.from));
            graph.add_node(graph_node(network, target));
            graph.add_edge(
                &network.claims[&edge.from].path(),
                &network.claims[target].path(),
                edge.kind().token(),
            );
        }
    }
    graph
}

fn dependencies(writer: &mut Writer, network: &Network) -> Result<()> {
    let overview = ecosystem_graph(network, |_| true).render(
        "Current relationships between indexed projects",
        "Dependents above, dependencies below. Only relationships that resolve to an indexed claim are drawn; the table lists all of them.",
    );
    let foundations = ecosystem_graph(network, |edge| {
        edge.targets()
            .iter()
            .any(|target| is_foundation(&network.claims[target]))
    })
    .render(
        "Libraries, frameworks and what uses them",
        "Every current relationship that lands on a library or framework.",
    );
    let mut used_by: Vec<UsedBy> = network
        .listed()
        .into_iter()
        .map(|claim| UsedBy {
            name: claim.name().to_owned(),
            path: claim.path(),
            count: network.used_by(&claim.key).len(),
            type_label: claim
                .manifest
                .as_ref()
                .map(|manifest| manifest.project.project_type.label().to_owned())
                .unwrap_or_default(),
        })
        .filter(|item| item.count > 0)
        .collect();
    used_by.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.name.cmp(&right.name))
    });
    let view = DependenciesView {
        overview,
        foundations,
        rows: network
            .edges
            .iter()
            .map(|edge| EdgeRow {
                from: network.claims[&edge.from].name().to_owned(),
                from_path: claim_path(&edge.from),
                kind: edge.kind().token().to_owned(),
                item: views::relationship_item(network, edge),
            })
            .collect(),
        used_by,
        unresolved: network
            .edges
            .iter()
            .filter(|edge| edge.resolution == Resolution::Unresolved)
            .count(),
    };
    let path = writer.view("dependencies", &view)?;
    writer.page(
        "dependencies/_index.md",
        &Front {
            title: "Dependencies",
            description: "What requires, recommends, conflicts with, is compatible with and replaces what, read from each claim's current release.",
            template: "dependencies.html",
            view: Some(&path),
        },
    )
}

// Capabilities --------------------------------------------------------------------------------

#[derive(Serialize)]
struct CapabilityRow {
    capability: String,
    path: String,
    providers: Vec<Link>,
    references: Vec<ReferenceRow>,
}

#[derive(Serialize)]
struct ReferenceRow {
    name: String,
    path: String,
    kind: String,
}

fn capability_rows(network: &Network) -> Vec<CapabilityRow> {
    network
        .capabilities
        .iter()
        .map(|(capability, entry)| CapabilityRow {
            capability: capability.clone(),
            path: format!("capabilities/{}/", capability_slug(capability)),
            providers: entry
                .providers
                .iter()
                .map(|key| Link {
                    label: network.claims[key].name().to_owned(),
                    url: claim_path(key),
                })
                .collect(),
            references: entry
                .references
                .iter()
                .map(|(key, kind)| ReferenceRow {
                    name: network.claims[key].name().to_owned(),
                    path: claim_path(key),
                    kind: kind.token().to_owned(),
                })
                .collect(),
        })
        .collect()
}

fn capabilities(writer: &mut Writer, network: &Network) -> Result<()> {
    let rows = capability_rows(network);
    let path = writer.view("capabilities", &rows)?;
    writer.page(
        "capabilities/_index.md",
        &Front {
            title: "Capabilities",
            description: "Named capabilities current releases provide, and what requires, recommends or conflicts with them. Capability names are published by projects; this index does not vouch for them.",
            template: "capabilities.html",
            view: Some(&path),
        },
    )?;
    for row in rows {
        let slug = capability_slug(&row.capability);
        let path = writer.view(&format!("capabilities/{slug}"), &row)?;
        writer.page(
            &format!("capabilities/{slug}.md"),
            &Front {
                title: &row.capability,
                description: &format!(
                    "{}, {}.",
                    plural(row.providers.len(), "provider", "providers"),
                    plural(row.references.len(), "reference", "references")
                ),
                template: "capability.html",
                view: Some(&path),
            },
        )?;
    }
    Ok(())
}

// Gaps ----------------------------------------------------------------------------------------

#[derive(Serialize)]
struct GapRow {
    by: String,
    title: String,
    detail: String,
    names: Vec<String>,
    urls: Vec<String>,
    references: Vec<ReferenceRow>,
}

#[derive(Serialize)]
struct Candidate {
    url: String,
    references: usize,
    command: String,
}

#[derive(Serialize)]
struct GapsView {
    gaps: Vec<GapRow>,
    candidates: Vec<Candidate>,
}

fn gaps(writer: &mut Writer, network: &Network) -> Result<()> {
    let gaps = network
        .gaps
        .iter()
        .map(|gap| {
            let (by, title, detail) = match &gap.key {
                GapKey::Project { project } => (
                    "project",
                    gap.names.iter().next().cloned().unwrap_or_else(|| project.to_string()),
                    format!("DreamWeave project id {project}. No site this index reads publishes it."),
                ),
                GapKey::Capability { capability } => (
                    "capability",
                    capability.clone(),
                    "A capability no current release in this index provides.".to_owned(),
                ),
                GapKey::Url { url } => (
                    "url",
                    gap.names.iter().next().cloned().unwrap_or_else(|| url.clone()),
                    "No DreamWeave identity published; grouped by the URL the references give.".to_owned(),
                ),
                GapKey::Name { name } => (
                    "name",
                    name.clone(),
                    "Only a name. References are grouped by identical text, which says nothing about whether they mean the same thing.".to_owned(),
                ),
            };
            GapRow {
                by: by.to_owned(),
                title,
                detail,
                names: gap.names.iter().cloned().collect(),
                urls: gap.urls.iter().cloned().collect(),
                references: gap
                    .references
                    .iter()
                    .map(|(key, kind)| ReferenceRow { name: network.claims[key].name().to_owned(), path: claim_path(key), kind: kind.token().to_owned() })
                    .collect(),
            }
        })
        .collect();
    let view = GapsView {
        gaps,
        candidates: network
            .candidates()
            .into_iter()
            .map(|(url, references)| Candidate {
                command: format!("cargo network inspect {url}"),
                url,
                references,
            })
            .collect(),
    };
    let path = writer.view("gaps", &view)?;
    writer.page(
        "gaps/_index.md",
        &Front {
            title: "Network gaps",
            description: "Relationships that lead outside the network: targets no site this index reads publishes. The boundary between the DreamWeave network and the rest of the modding world.",
            template: "gaps.html",
            view: Some(&path),
        },
    )
}

// Health and origins --------------------------------------------------------------------------

#[derive(Serialize)]
struct SourceRow {
    url: String,
    health: String,
    problem: Option<String>,
    method: Option<String>,
    origin_path: Option<String>,
    last_attempt: String,
    last_success: Option<String>,
    trail: Vec<Fact>,
}

#[derive(Serialize)]
struct OriginRow {
    id: String,
    path: String,
    label: String,
    address: String,
    index_url: String,
    health: String,
    health_label: String,
    problem: Option<String>,
    issues: Vec<String>,
    generator: Option<String>,
    projects: usize,
    first_observed: String,
    last_attempt: String,
    last_success: Option<String>,
    sources: Vec<String>,
    redirects: Vec<String>,
    previous: Vec<Fact>,
}

#[derive(Serialize)]
struct ClaimProblem {
    name: String,
    path: Option<String>,
    origin_label: String,
    health: String,
    health_label: String,
    problem: Option<String>,
    last_success: Option<String>,
}

#[derive(Serialize)]
struct HealthView {
    stats: Vec<Stat>,
    sources: Vec<SourceRow>,
    origins: Vec<OriginRow>,
    problems: Vec<ClaimProblem>,
    counts: Vec<Fact>,
    generators: Vec<Fact>,
}

fn origin_row(network: &Network, origin: &crate::state::OriginRecord) -> OriginRow {
    OriginRow {
        id: origin.id.clone(),
        path: views::origin_path(&origin.id),
        label: origin_label(origin),
        address: site_address(&origin.index_url),
        index_url: origin.index_url.clone(),
        health: origin.health.token().to_owned(),
        health_label: origin.health.label().to_owned(),
        problem: origin.problem.clone(),
        issues: origin.issues.clone(),
        generator: origin.generator.clone(),
        projects: network
            .claims
            .keys()
            .filter(|key| key.origin == origin.id)
            .count(),
        first_observed: time_label(&origin.first_observed),
        last_attempt: time_label(&origin.last_attempt),
        last_success: origin.last_success.as_deref().map(time_label),
        sources: origin.sources.clone(),
        redirects: origin.redirects.clone(),
        previous: origin
            .previous
            .iter()
            .map(|moved| {
                Fact::text(
                    &time_label(&moved.observed_at),
                    format!(
                        "replaced {} ({})",
                        site_address(&moved.index_url),
                        moved.evidence
                    ),
                )
            })
            .collect(),
    }
}

fn health(writer: &mut Writer, network: &Network) -> Result<()> {
    let mut generators: BTreeMap<String, usize> = BTreeMap::new();
    for origin in network.origins.values() {
        *generators
            .entry(
                origin
                    .generator
                    .clone()
                    .unwrap_or_else(|| "not stated".to_owned()),
            )
            .or_default() += 1;
    }
    let view = HealthView {
        stats: views::network_stats(network),
        sources: network
            .sources
            .iter()
            .map(|source| SourceRow {
                url: source.url.clone(),
                health: format!("{:?}", source.health).to_lowercase(),
                problem: source.problem.clone(),
                method: source.method.clone(),
                origin_path: source.origin.as_deref().map(views::origin_path),
                last_attempt: time_label(&source.last_attempt),
                last_success: source.last_success.as_deref().map(time_label),
                trail: source
                    .trail
                    .iter()
                    .map(|attempt| Fact::code(&attempt.outcome, attempt.url.clone()))
                    .collect(),
            })
            .collect(),
        origins: network
            .origins
            .values()
            .map(|origin| origin_row(network, origin))
            .collect(),
        problems: network
            .claims
            .values()
            .filter(|claim| claim.record.health != ClaimHealth::Current)
            .map(|claim| ClaimProblem {
                name: claim.name().to_owned(),
                path: claim.manifest.is_some().then(|| claim.path()),
                origin_label: network
                    .origins
                    .get(&claim.key.origin)
                    .map_or_else(|| claim.key.origin.clone(), origin_label),
                health: claim.record.health.token().to_owned(),
                health_label: claim.record.health.label().to_owned(),
                problem: claim.record.problem.clone(),
                last_success: claim.record.last_success.as_deref().map(time_label),
            })
            .collect(),
        counts: views::status_counts(network)
            .into_iter()
            .map(|(health, count)| Fact::text(&health, count.to_string()))
            .collect(),
        generators: generators
            .into_iter()
            .map(|(generator, count)| Fact::text(&generator, plural(count, "site", "sites")))
            .collect(),
    };
    let path = writer.view("health", &view)?;
    writer.page(
        "health/_index.md",
        &Front {
            title: "Network health",
            description: "The result of the most recent crawl: every enrolled source, every site, every claim that is not current, and why.",
            template: "health.html",
            view: Some(&path),
        },
    )
}

#[derive(Serialize)]
struct OriginView {
    origin: OriginRow,
    cards: Vec<Card>,
    others: Vec<ClaimProblem>,
    events: Vec<EventRow>,
}

fn origins(writer: &mut Writer, network: &Network) -> Result<()> {
    let rows: Vec<OriginRow> = network
        .origins
        .values()
        .map(|origin| origin_row(network, origin))
        .collect();
    let path = writer.view("origins", &rows)?;
    writer.page(
        "origins/_index.md",
        &Front {
            title: "Sites",
            description: "Every site this index reads, and what it publishes.",
            template: "origins.html",
            view: Some(&path),
        },
    )?;
    for origin in network.origins.values() {
        let view = OriginView {
            origin: origin_row(network, origin),
            cards: cards(network, |claim| claim.key.origin == origin.id),
            others: network
                .claims
                .values()
                .filter(|claim| claim.key.origin == origin.id && !claim.is_listed())
                .map(|claim| ClaimProblem {
                    name: claim.name().to_owned(),
                    path: claim.manifest.is_some().then(|| claim.path()),
                    origin_label: origin_label(origin),
                    health: claim.record.health.token().to_owned(),
                    health_label: claim.record.health.label().to_owned(),
                    problem: claim.record.problem.clone(),
                    last_success: claim.record.last_success.as_deref().map(time_label),
                })
                .collect(),
            events: network
                .events
                .iter()
                .filter(|event| event.origin == origin.id)
                .map(|event| event_row(network, event))
                .take(20)
                .collect(),
        };
        let label = origin_label(origin);
        let path = writer.view(&format!("origins/{}", origin.id), &view)?;
        writer.page(
            &format!("origins/{}.md", origin.id),
            &Front {
                title: &label,
                description: &format!(
                    "The site at {} and the claims it publishes.",
                    site_address(&origin.index_url)
                ),
                template: "origin.html",
                view: Some(&path),
            },
        )?;
    }
    Ok(())
}

#[derive(Serialize)]
struct ConflictView {
    project: String,
    path: String,
    claims: Vec<IdentityClaim>,
}

fn conflicts(writer: &mut Writer, network: &Network) -> Result<()> {
    let rows: Vec<ConflictView> = network
        .conflicts
        .iter()
        .map(|conflict| ConflictView {
            project: conflict.project.to_string(),
            path: format!("projects/{}/", conflict.project),
            claims: identity_claims(network, &conflict.project),
        })
        .collect();
    let path = writer.view("conflicts", &rows)?;
    writer.page(
        "conflicts/_index.md",
        &Front {
            title: "Identity conflicts",
            description: "Project ids published by more than one site. Neither claim wins here: ids are chosen, not allocated, and this index is not the authority that could decide.",
            template: "conflicts.html",
            view: Some(&path),
        },
    )
}

// Public data ---------------------------------------------------------------------------------

#[derive(Serialize)]
struct SearchRecord {
    title: String,
    url: String,
    kind: String,
    summary: String,
    badge: String,
    text: String,
    fields: BTreeMap<String, Vec<String>>,
}

#[derive(Serialize)]
struct Search {
    format: &'static str,
    format_version: u32,
    records: Vec<SearchRecord>,
}

fn claim_search(network: &Network, claim: &Claim) -> SearchRecord {
    let manifest = claim
        .manifest
        .as_ref()
        .expect("searchable claims hold a manifest");
    let project = &manifest.project;
    let release = claim.current_release();
    let mut fields: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut put = |field: &str, values: Vec<String>| {
        fields
            .entry(field.to_owned())
            .or_default()
            .extend(values.into_iter().map(|value| value.to_lowercase()));
    };
    put("type", vec![project.project_type.token().to_owned()]);
    put("status", vec![project.status.token().to_owned()]);
    put("game", vec![project.game.clone()]);
    put("tag", project.tags.clone());
    put(
        "maintainer",
        project
            .maintainers
            .iter()
            .map(|person| person.name.clone())
            .collect(),
    );
    put("license", project.license.clone().into_iter().collect());
    put("channel", manifest.channels.keys().cloned().collect());
    put("health", vec![claim.record.health.token().to_owned()]);
    put("id", vec![claim.key.project.to_string()]);
    put(
        "site",
        vec![
            network
                .origins
                .get(&claim.key.origin)
                .map_or_else(|| claim.key.origin.clone(), origin_label),
        ],
    );
    if let Some(release) = release {
        put(
            "runtime",
            release
                .runtimes
                .iter()
                .map(|(runtime, constraint)| format!("{runtime} {constraint}"))
                .collect(),
        );
        put(
            "lua",
            release
                .openmw()
                .and_then(|openmw| openmw.lua_api)
                .into_iter()
                .collect(),
        );
        put("provides", release.provides.clone());
        for kind in RelationshipKind::ALL {
            let targets: Vec<String> = release
                .relationships
                .iter()
                .filter(|relationship| relationship.kind == kind)
                .flat_map(|relationship| {
                    [
                        relationship.name.clone(),
                        relationship.capability.clone(),
                        relationship.project.as_ref().map(ToString::to_string),
                    ]
                })
                .flatten()
                .collect();
            put(kind.token(), targets);
        }
    }
    let mut text = vec![
        project.name.clone(),
        project.summary.clone().unwrap_or_default(),
    ];
    text.extend(fields.values().flatten().cloned());
    SearchRecord {
        title: project.name.clone(),
        url: claim.path(),
        kind: "project".to_owned(),
        summary: project.summary.clone().unwrap_or_default(),
        badge: format!(
            "{} · {}",
            project.project_type.token(),
            project.status.token()
        ),
        text: text.join(" ").to_lowercase(),
        fields,
    }
}

fn front_matter_field(text: &str, field: &str) -> Option<String> {
    let front = text.strip_prefix("+++")?.split("+++").next()?;
    let table: toml::Table = toml::from_str(front).ok()?;
    table.get(field)?.as_str().map(str::to_owned)
}

fn page_search(pages: &Path, records: &mut Vec<SearchRecord>) -> Result<()> {
    if !pages.exists() {
        return Ok(());
    }
    let mut pending = vec![pages.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "md") {
                files.push(path);
            }
        }
    }
    files.sort();
    for file in files {
        let text = fs::read_to_string(&file).with_context(|| format!("read {}", file.display()))?;
        let Some(title) = front_matter_field(&text, "title") else {
            continue;
        };
        let relative = file
            .strip_prefix(pages)?
            .to_string_lossy()
            .replace('\\', "/");
        let url = if relative.ends_with("_index.md") {
            relative.trim_end_matches("_index.md").to_owned()
        } else {
            format!("{}/", relative.trim_end_matches(".md"))
        };
        let summary = front_matter_field(&text, "description").unwrap_or_default();
        records.push(SearchRecord {
            text: format!("{title} {summary}").to_lowercase(),
            title,
            url,
            kind: "page".to_owned(),
            summary,
            badge: "documentation".to_owned(),
            fields: BTreeMap::new(),
        });
    }
    Ok(())
}

fn atom(network: &Network, index: &IndexInfo) -> String {
    let escape = crate::graph::escape;
    let base = index.url.trim_end_matches('/');
    let updated = network
        .observed_at
        .clone()
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_owned());
    let mut feed = String::new();
    let _ = write!(
        feed,
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<feed xmlns=\"http://www.w3.org/2005/Atom\">\n<title>{} updates</title>\n<subtitle>Changes this index observed in independently published DreamWeave manifests.</subtitle>\n<id>{base}/updates/</id>\n<link rel=\"self\" href=\"{base}/network-data/updates.xml\"/>\n<link rel=\"alternate\" href=\"{base}/updates/\"/>\n<updated>{updated}</updated>\n",
        escape(&index.name)
    );
    for event in network.events.iter().take(100) {
        let row = event_row(network, event);
        let _ = write!(
            feed,
            "<entry>\n<title>{}: {}</title>\n<id>{base}/updates/{}/</id>\n<link href=\"{base}/updates/{}/\"/>\n<updated>{}</updated>\n<author><name>{}</name></author>\n<summary>{} Observed by this index at {}; published by {}.</summary>\n</entry>\n",
            escape(&event.name),
            escape(&row.headline),
            event.id,
            event.id,
            event.observed_at,
            escape(&row.origin_label),
            escape(&row.tags.join(", ")),
            escape(&row.observed_label),
            escape(&row.origin_label),
        );
    }
    feed.push_str("</feed>\n");
    feed
}

fn public_data(writer: &Writer, network: &Network, index: &IndexInfo, pages: &Path) -> Result<()> {
    let catalog = serde_json::to_value(catalog::catalog(network, index))?;
    catalog::check(&catalog)?;
    writer.json(&format!("{}/catalog.json", super::DATA_DIRECTORY), &catalog)?;
    let events = serde_json::to_value(catalog::events(network, index))?;
    catalog::check(&events)?;
    writer.json(&format!("{}/events.json", super::DATA_DIRECTORY), &events)?;

    let mut records: Vec<SearchRecord> = network
        .listed()
        .into_iter()
        .map(|claim| claim_search(network, claim))
        .collect();
    records.extend(
        capability_rows(network)
            .into_iter()
            .map(|row| SearchRecord {
                text: row.capability.to_lowercase(),
                title: row.capability.clone(),
                url: row.path,
                kind: "capability".to_owned(),
                summary: format!(
                    "{} · {}",
                    plural(row.providers.len(), "provider", "providers"),
                    plural(row.references.len(), "reference", "references")
                ),
                badge: "capability".to_owned(),
                fields: BTreeMap::new(),
            }),
    );
    records.extend(network.origins.values().map(|origin| SearchRecord {
        text: format!("{} {}", origin_label(origin), origin.index_url).to_lowercase(),
        title: origin_label(origin),
        url: views::origin_path(&origin.id),
        kind: "site".to_owned(),
        summary: site_address(&origin.index_url),
        badge: format!("site · {}", origin.health.label()),
        fields: BTreeMap::new(),
    }));
    page_search(pages, &mut records)?;
    writer.json(
        &format!("{}/search.json", super::DATA_DIRECTORY),
        &Search {
            format: "dreamweave-network-search",
            format_version: 1,
            records,
        },
    )?;
    writer.file(
        &format!("{}/updates.xml", super::DATA_DIRECTORY),
        atom(network, index).as_bytes(),
    )
}

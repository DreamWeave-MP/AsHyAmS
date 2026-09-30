//! AsHyAmS, an index of the DreamWeave network: reads sites that publish the DreamWeave protocol
//! (`schema_version` "2"), remembers what they said, and renders it as a static site.

pub mod address;
pub mod catalog;
pub mod config;
pub mod crawl;
pub mod diff;
pub mod discovery;
pub mod events;
pub mod fetch;
pub mod graph;
pub mod inspect;
pub mod markdown;
pub mod network;
pub mod policy;
pub mod protocol;
pub mod site;
pub mod sitecheck;
pub mod state;
pub mod version;

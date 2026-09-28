//! The DreamWeave Network index: reads sites that publish the DreamWeave protocol
//! (`schema_version` "2"), remembers what they said, and renders it as a static site.

pub mod address;
pub mod fetch;
pub mod policy;
pub mod version;

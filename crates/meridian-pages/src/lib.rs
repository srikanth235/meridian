//! Pages subsystem.
//!
//! Watches `<workflow_dir>/pages/<slug>/{page.tsx,meta.toml}`. Each folder is
//! one LLM-authored React module that renders inside a sandboxed iframe in
//! the renderer. The slug (folder name) is the immutable identity; the
//! `meta.toml` `title` is mutable.
//!
//! This crate does *not* execute the page — execution happens in the
//! renderer's iframe. The crate's job is:
//!  - discover page folders on disk
//!  - parse each `meta.toml`
//!  - persist a thin shadow into SQLite for the sidebar
//!  - serve `page.tsx` source on demand to the renderer
//!  - drive a natural-language → inbox-spec workflow that lets a coding
//!    harness author new pages, mirroring the automations subsystem.

pub mod meta;
pub mod nl;
pub mod registry;
pub mod service;

pub use meta::{Meta, DEFAULT_META_VERSION};
pub use service::{PagesHandle, PagesService};

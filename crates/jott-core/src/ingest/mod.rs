//! Getting notes into np's index, and the consent that decides whether they can be embedded.
//!
//! The queue is the only path to "ingested" (§11): manual saves, autosave, the save on close, the
//! "ingest this" menu item and "ingest everything waiting" all run through it, so there is one
//! definition of the word.

pub mod consent;
pub mod queue;

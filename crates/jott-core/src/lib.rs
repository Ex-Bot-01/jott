//! Jott's core: the vault on disk and the `np` child process.
//!
//! Nothing in here knows a window exists. The webview asks for things through the
//! shell's commands; every decision about files, hashes and the protocol lives here
//! so it can be tested without a display server.

pub mod app_config;

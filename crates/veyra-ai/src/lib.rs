//! Local AI building blocks. The engine is llama.cpp's `llama-server` run as a supervised child process bound to loopback
//! (crash isolation, real memory release on unload, clean cancellation). Nothing in this crate calls a remote AI provider.
pub mod catalog;
pub mod context;
pub mod download;
pub mod engine;
pub mod hardware;

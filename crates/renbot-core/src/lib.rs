//! Logica di renbot, indipendente dalla GUI.

pub mod cache;
pub mod config;
pub mod http;
pub mod matcher;
pub mod parser;
pub mod providers;
pub mod renamer;
pub mod scanner;

pub mod workflow;

#[cfg(any(test, feature = "test-util"))]
pub mod testing;

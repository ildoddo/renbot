//! Logica di renbot, indipendente dalla GUI.

pub mod cache;
pub mod config;
pub mod http;
pub mod matcher;
pub mod parser;
pub mod providers;
pub mod renamer;
pub mod scanner;

#[cfg(test)]
pub(crate) mod testing;

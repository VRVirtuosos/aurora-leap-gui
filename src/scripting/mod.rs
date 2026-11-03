//! Scripting and automated testing module using Rhai.

pub mod catalog;
pub mod engine;
pub mod models;
pub mod runner;
pub mod syntax;

pub use catalog::ScriptCatalog;
pub use engine::{ScriptContext, ScriptEvent};
pub use models::{AssertionRecord, LogLevel, ScriptLogMessage, ScriptStatus, TestReport};
pub use runner::ScriptRunner;
pub use syntax::highlight_rhai_script;

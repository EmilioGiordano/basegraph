//! Parser module for CodeGraph

pub mod rust;

use thiserror::Error;

pub trait LanguageParser {
    fn parse_source(&self, source: &str, file: &str)
        -> Result<Vec<crate::model::Node>, ParseError>;
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("syntax error: {0}")]
    Syntax(String),
}

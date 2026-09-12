use std::borrow::Cow;

/// Errors produced when building full-text search expressions.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FtsError {
    /// The text search configuration name is not a bare lowercase identifier.
    #[error(
        "invalid text search configuration name {0:?}: expected a bare lowercase identifier such as `english`"
    )]
    InvalidConfigName(String),
}

/// The name of a Postgres text search configuration, such as `english`.
///
/// sea-query's `PgFunc` tsquery constructors only accept a `regconfig` OID, so
/// the name is rendered into the SQL as a literal rather than bound as a
/// parameter. The name is therefore validated on construction: it must be a
/// bare lowercase identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsConfig(Cow<'static, str>);

impl TsConfig {
    /// The `english` configuration.
    pub const ENGLISH: TsConfig = TsConfig(Cow::Borrowed("english"));
    /// The `simple` configuration, which applies no stemming or stopwords.
    pub const SIMPLE: TsConfig = TsConfig(Cow::Borrowed("simple"));

    /// Build a configuration from a name, validating it is a bare lowercase
    /// identifier (`^[a-z_][a-z0-9_]*$`).
    pub fn new(name: &str) -> Result<TsConfig, FtsError> {
        if is_bare_lowercase_identifier(name) {
            Ok(TsConfig(Cow::Owned(name.to_owned())))
        } else {
            Err(FtsError::InvalidConfigName(name.to_owned()))
        }
    }

    /// The configuration name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_bare_lowercase_identifier(name: &str) -> bool {
    let mut chars = name.chars();

    match chars.next() {
        Some(first) if first.is_ascii_lowercase() || first == '_' => {}
        _ => return false,
    }

    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

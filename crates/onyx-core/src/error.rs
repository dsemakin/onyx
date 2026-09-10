use std::fmt;

/// Errors returned when reading a document.
///
/// These are values rather than panics on purpose: this crate parses files that arrive
/// from untrusted sources — a diary someone was emailed, or restored from a backup of
/// unknown origin — and aborting the host process is never an acceptable response to a
/// malformed one.
///
/// The `Display` and `Error` implementations are written out rather than derived. A
/// derive macro for four variants is a build-time dependency, a proc-macro compilation,
/// and a supply-chain surface, in exchange for thirty lines.
#[derive(Debug)]
pub enum Error {
    /// The input was not well-formed JSON.
    Json(crate::json::ParseError),

    /// A well-formed JSON object that is not an ONYX document.
    ///
    /// Identity comes from the `format` member, never from the file name and never from
    /// `$schema` — documents in the wild carry several different `$schema` values and all
    /// of them are valid.
    NotOnyx {
        found: String,
        expected: &'static str,
    },

    /// `specVersion` was present but not semver-shaped.
    MalformedVersion(String),

    /// No chain of migration manifests connects the document version to the target.
    NoMigrationPath { from: String, to: String },

    /// A downgrade had members to park and nowhere to put them, because the document's
    /// `extensions` are not an object. Refusing keeps them; continuing would drop them.
    CannotDemote {
        from: String,
        to: String,
        reason: String,
    },

    /// An upgrade could not put a parked member back where it came from.
    ///
    /// The members are still parked, so nothing is lost — but the document is not the one
    /// that was asked for, and saying so is the only honest answer.
    CannotPromote {
        from: String,
        to: String,
        reason: String,
    },

    /// A migration manifest could not be read.
    ///
    /// Only reachable through [`migrate_with`](crate::migrate_with), where the manifests
    /// come from the caller. The ones this build embeds are its own and are checked in CI.
    MalformedManifest(String),

    /// A newer MAJOR may have changed the meaning of members this build thinks it
    /// understands. Refusing is the safe response; misreading someone's history is not.
    UnsupportedMajor { found: u64, supported: u64 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(f, "not valid JSON: {error}"),
            Self::NotOnyx { found, expected } => write!(
                f,
                "not an Onyx document: `format` was {found:?}, expected {expected:?}"
            ),
            Self::MalformedVersion(version) => {
                write!(f, "`specVersion` is not semver-shaped: {version:?}")
            }
            Self::UnsupportedMajor { found, supported } => write!(
                f,
                "document is spec major {found}, this build understands major {supported}"
            ),
            Self::NoMigrationPath { from, to } => {
                write!(f, "no migration manifest connects {from} to {to}")
            }
            Self::MalformedManifest(why) => write!(f, "a migration manifest is unusable: {why}"),
            Self::CannotPromote { from, to, reason } => {
                write!(f, "cannot migrate {from} up to {to}: {reason}")
            }
            Self::CannotDemote { from, to, reason } => {
                write!(f, "cannot migrate {from} down to {to}: {reason}")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<crate::json::ParseError> for Error {
    fn from(error: crate::json::ParseError) -> Self {
        Self::Json(error)
    }
}

impl Error {
    /// A stable identifier for this failure, in the same `category/rule` namespace the
    /// validator uses.
    ///
    /// Corpus cases name a rule rather than a message so that implementations are held to
    /// the behaviour rather than to this crate's wording — and so that a failure at the
    /// identity gate and a failure during validation can be expressed the same way.
    pub fn rule(&self) -> &'static str {
        match self {
            Self::Json(_) => "document/unreadable",
            Self::NotOnyx { .. } => "document/not-onyx",
            Self::MalformedVersion(_) => "document/malformed-version",
            Self::UnsupportedMajor { .. } => "document/unsupported-major",
            Self::NoMigrationPath { .. } => "migration/no-path",
            Self::MalformedManifest(_) => "migration/malformed-manifest",
            Self::CannotDemote { .. } => "migration/cannot-demote",
            Self::CannotPromote { .. } => "migration/cannot-promote",
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

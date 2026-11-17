//! Reading the backup of another hosting panel into one common model (T15.01 - T15.03).
//!
//! ```text
//! backup archive --Archive::scan--> table of contents + small files
//!                --ImportSource--> ImportModel (account, domains, databases, mail, DNS, ...)
//! ```
//!
//! The model names what the account has; the files and dumps stay in the archive and are
//! streamed out by the executor. Reading never writes to disk, and every path a source puts
//! in the model is an [`ArchivePath`] that cannot leave the archive.

mod archive;
pub mod cpanel;
pub mod cyberpanel;
mod model;
mod plan;
mod zone;

pub use archive::{Archive, HOME, Limits};
pub use model::*;
pub use plan::{
    Action, Conflict, Existing, Include, OnConflict, Options, Plan, Resolution, Skipped, Step, plan,
};
pub use zone::parse_zone;

use std::io::Read;

/// Why a backup could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ImportError {
    /// The archive is not a readable tar (or gzip) file.
    #[error("the archive cannot be read: {0}")]
    Archive(String),
    /// A path in the archive would leave it.
    #[error("unsafe path in the archive: {0}")]
    UnsafePath(String),
    /// The archive passes a limit.
    #[error("the archive is too big: {0}")]
    TooBig(String),
    /// No source recognises the archive.
    #[error("this is not a backup of a supported panel")]
    Unrecognised,
    /// The archive is of a known panel but its content is not usable.
    #[error("invalid backup: {0}")]
    Invalid(String),
}

/// A panel whose backups can be read.
pub trait ImportSource {
    /// The panel.
    fn kind(&self) -> SourceKind;
    /// Whether the archive looks like this panel's.
    fn detect(&self, archive: &Archive) -> bool;
    /// Which paths under the home directory the reader wants listed or read.
    fn keep_home(&self, path: &str) -> bool;
    /// Reads the model.
    ///
    /// # Errors
    /// [`ImportError`].
    fn parse(&self, archive: &Archive) -> Result<ImportModel, ImportError>;
}

/// cPanel.
pub struct Cpanel;
/// CyberPanel.
pub struct Cyberpanel;

impl ImportSource for Cpanel {
    fn kind(&self) -> SourceKind {
        SourceKind::Cpanel
    }
    fn detect(&self, a: &Archive) -> bool {
        cpanel::detect(a)
    }
    fn keep_home(&self, path: &str) -> bool {
        cpanel::keep_home(path)
    }
    fn parse(&self, a: &Archive) -> Result<ImportModel, ImportError> {
        cpanel::parse(a)
    }
}

impl ImportSource for Cyberpanel {
    fn kind(&self) -> SourceKind {
        SourceKind::Cyberpanel
    }
    fn detect(&self, a: &Archive) -> bool {
        cyberpanel::detect(a)
    }
    fn keep_home(&self, path: &str) -> bool {
        cyberpanel::keep_home(path)
    }
    fn parse(&self, a: &Archive) -> Result<ImportModel, ImportError> {
        cyberpanel::parse(a)
    }
}

/// Every source this build can read.
#[must_use]
pub fn sources() -> Vec<Box<dyn ImportSource>> {
    vec![Box::new(Cpanel), Box::new(Cyberpanel)]
}

/// Reads a backup of any supported panel: scans the archive and lets the source that
/// recognises it read it. The model is checked before it is returned.
///
/// # Errors
/// [`ImportError`].
pub fn read_backup<R: Read>(reader: R, limits: Limits) -> Result<ImportModel, ImportError> {
    let all = sources();
    let archive = Archive::scan(reader, limits, |p| all.iter().any(|s| s.keep_home(p)))?;
    let source = all
        .iter()
        .find(|s| s.detect(&archive))
        .ok_or(ImportError::Unrecognised)?;
    let model = source.parse(&archive)?;
    model.validate()?;
    Ok(model)
}

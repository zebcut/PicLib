//! Import de photos et de vidéos dans le catalogue.
//!
//! Deux modes : **par référence**, les fichiers restent où ils
//! sont et le catalogue pointe vers eux ; **par copie**, ils sont copiés,
//! vérifiés et rangés par date, et le catalogue pointe vers les copies.

mod copy;
mod folder_tree;
mod layout;
mod outcome;
mod recovery;
mod session;
mod sidecars;
mod source;

pub use copy::{CopyOptions, DestinationRefusal, check_destination, group_capture_time};
pub use layout::{Arrangement, DatePart, DestinationLayout, PlannedFolder, planned_tree};
pub use outcome::{ImportError, ImportOptions, ImportReport};
pub use session::{RecoveredCopy, import_by_copy, import_by_reference, register_recovered_copies};
pub use source::{SourceFile, SourceFileStatus, read_source};
// Relecture des métadonnées de médias déjà au catalogue (`rereading`) : mêmes
// sources, même ordre de priorité qu'à l'import.
pub(crate) use recovery::recover;
pub(crate) use sidecars::Sidecars;

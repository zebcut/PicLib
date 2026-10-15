//! Retouche dans un éditeur externe :
//! PicLib ne retouche pas ; il confie les photos à un autre logiciel (DxO
//! PhotoLab, Photoshop...) et ajoute au catalogue ce que celui-ci produit,
//! empilé avec l'original.

mod editors;
mod follow;
mod send;
mod versions;

pub use editors::{
    CustomEditor, Editor, EditorSettings, Preset, application_name, available, custom_id,
    default_editor, editor_name,
};
pub use follow::{
    FOLLOW_DURATION_MS, FollowPlan, FollowedPhoto, derived_versions, files_in, known_media,
    new_versions, plan as follow_plan, stop as stop_following, touch,
};
pub use send::{EditCandidate, EditPlan, SendError, SendOutcome, SendRequest, plan, send};
pub use versions::{InProgress, NewVersion, add_versions, is_in_progress, mark_in_progress};

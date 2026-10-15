//! Identifiants typés des éléments du catalogue.
//!
//! Chaque identifiant a son propre type : le compilateur refuse de passer un
//! identifiant de dossier là où un identifiant de média est attendu, alors que
//! ce seraient deux simples nombres dans la base.

/// Déclare un identifiant typé qui enveloppe un entier de la base.
macro_rules! typed_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(i64);

        impl $name {
            /// Crée un identifiant à partir de sa valeur dans la base.
            #[must_use]
            pub const fn new(value: i64) -> Self {
                Self(value)
            }

            /// Valeur de l'identifiant dans la base.
            #[must_use]
            pub const fn get(self) -> i64 {
                self.0
            }
        }
    };
}

typed_id!(
    /// Identifiant d'un dossier du catalogue.
    FolderId
);
typed_id!(
    /// Identifiant d'une photo ou d'une vidéo du catalogue.
    MediaId
);
typed_id!(
    /// Identifiant d'une session d'import.
    ImportId
);
typed_id!(
    /// Identifiant d'un mot-clé.
    KeywordId
);
typed_id!(
    /// Identifiant d'un jeu de mots-clés.
    KeywordSetId
);
typed_id!(
    /// Identifiant d'un modèle de métadonnées.
    MetadataTemplateId
);
typed_id!(
    /// Identifiant d'un préréglage de filtres.
    FilterPresetId
);
typed_id!(
    /// Identifiant d'une collection ou d'un dossier de collections.
    CollectionId
);
typed_id!(
    /// Identifiant d'une pile.
    StackId
);

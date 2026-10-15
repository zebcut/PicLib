//! Catalogue SQLite : ouverture, migrations, sauvegarde et accès aux données.
//!
//! Le catalogue est un fichier SQLite dans le dossier de travail (voir
//! [`crate::workspace`]). À l'ouverture :
//!
//! 1. on vérifie que le fichier est vide ou est bien un catalogue PicLib ;
//! 2. on refuse un catalogue créé par une version plus récente de PicLib ;
//! 3. si des migrations sont à appliquer à un catalogue existant, on en fait
//!    d'abord une sauvegarde vérifiée ;
//! 4. on applique les migrations, chacune dans sa transaction.

pub mod auto_backup;
mod backup;
mod connection;
mod error;
pub mod migrations;
pub mod repositories;
mod sql;

use std::path::{Path, PathBuf};

use rusqlite::{Connection, Transaction};

pub use error::{BackupError, CatalogError, OpenError};

use crate::workspace::Workspace;
use connection::FileIdentity;
use migrations::Migration;

/// Que faire si le fichier du catalogue n'existe pas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    /// Créer un catalogue vide (premier lancement).
    CreateIfMissing,
    /// Signaler une erreur : un catalogue connu qui a disparu ne doit pas être
    /// remplacé silencieusement par un catalogue vide.
    MustExist,
}

/// Catalogue ouvert, prêt à l'emploi.
#[derive(Debug)]
pub struct Catalog {
    connection: Connection,
    path: PathBuf,
    schema_version: u32,
}

impl Catalog {
    /// Ouvre (ou crée) le catalogue du dossier de travail et le met à jour.
    ///
    /// # Errors
    ///
    /// Voir [`OpenError`] : catalogue introuvable, illisible, étranger, trop
    /// récent, ou échec de la sauvegarde ou d'une migration. Dans tous les cas,
    /// le fichier existant n'est pas modifié.
    pub fn open(workspace: &Workspace, mode: OpenMode) -> Result<Self, OpenError> {
        Self::open_with_migrations(workspace, mode, migrations::MIGRATIONS)
    }

    /// Vérifie, sans rien modifier, que le dossier de travail contient un
    /// catalogue PicLib que cette version sait ouvrir, et renvoie sa version
    /// de structure. Sert à contrôler un catalogue avant de basculer dessus.
    ///
    /// # Errors
    ///
    /// Mêmes erreurs d'identification que [`Catalog::open`] : catalogue
    /// introuvable, illisible, étranger ou trop récent.
    pub fn inspect(workspace: &Workspace) -> Result<u32, OpenError> {
        let path = workspace.catalog_path();
        if !path.is_file() {
            return Err(OpenError::Missing { path });
        }
        let connection = connection::open_read_only(&path)?;
        let version = Self::identify_version(&connection, &path)?;
        let supported = migrations::latest_version(migrations::MIGRATIONS);
        if version > supported {
            return Err(OpenError::TooNew {
                path,
                found: version,
                supported,
            });
        }
        Ok(version)
    }

    /// Version de structure d'un fichier vide (0) ou d'un catalogue PicLib.
    fn identify_version(connection: &Connection, path: &Path) -> Result<u32, OpenError> {
        match connection::identify(connection, path)? {
            FileIdentity::Empty => Ok(0),
            FileIdentity::PicLibCatalog => {
                migrations::current_version(connection).map_err(|source| OpenError::Unreadable {
                    path: path.to_path_buf(),
                    source,
                })
            }
        }
    }

    fn open_with_migrations(
        workspace: &Workspace,
        mode: OpenMode,
        migrations: &[Migration],
    ) -> Result<Self, OpenError> {
        let path = workspace.catalog_path();
        if mode == OpenMode::MustExist && !path.is_file() {
            return Err(OpenError::Missing { path });
        }

        let mut connection = connection::open(&path, mode == OpenMode::CreateIfMissing)?;
        let current_version = Self::identify_version(&connection, &path)?;
        let supported_version = migrations::latest_version(migrations);
        if current_version > supported_version {
            return Err(OpenError::TooNew {
                path,
                found: current_version,
                supported: supported_version,
            });
        }

        connection::configure(&connection, &path)?;
        Self::migrate(&mut connection, workspace, migrations, current_version)?;
        Self::remove_folders_without_media(&mut connection);
        Self::remove_lone_stacks(&connection, supported_version);

        tracing::info!(path = %path.display(), schema_version = supported_version, "catalog opened");
        Ok(Self {
            connection,
            path,
            schema_version: supported_version,
        })
    }

    /// Retire les dossiers sans média laissés par un arrêt brutal ou une
    /// ancienne version (le catalogue ne garde jamais un dossier sans photo ni vidéo). Un échec est journalisé sans
    /// empêcher l'ouverture : le catalogue reste utilisable.
    fn remove_folders_without_media(connection: &mut rusqlite::Connection) {
        let result = connection
            .transaction()
            .map_err(CatalogError::from)
            .and_then(|transaction| {
                repositories::folders::remove_without_media(&transaction)?;
                transaction.commit().map_err(CatalogError::from)
            });
        if let Err(error) = result {
            tracing::warn!(%error, "cannot remove the folders without media");
        }
    }

    /// Retire les piles de moins de deux photos, gardées pendant la session
    /// précédente pour ses annulations (migration 27). Un échec est
    /// journalisé sans empêcher l'ouverture.
    fn remove_lone_stacks(connection: &rusqlite::Connection, version: u32) {
        if version < 27 {
            return;
        }
        if let Err(error) = repositories::stacks::remove_lone(connection) {
            tracing::warn!(%error, "cannot remove the stacks of less than two photos");
        }
    }

    /// Applique les migrations en attente, après une sauvegarde vérifiée si
    /// le catalogue contient déjà des données.
    fn migrate(
        connection: &mut Connection,
        workspace: &Workspace,
        migrations: &[Migration],
        current_version: u32,
    ) -> Result<(), OpenError> {
        let pending = migrations::pending(migrations, current_version);
        if pending.is_empty() {
            return Ok(());
        }

        // Un catalogue neuf (version 0) n'a rien à perdre : pas de sauvegarde.
        let backup = if current_version > 0 {
            Some(backup::create_verified_backup(
                connection,
                &workspace.backups_dir(),
                current_version,
            )?)
        } else {
            None
        };

        for migration in pending {
            tracing::info!(
                version = migration.version,
                description = migration.description,
                "applying catalog migration"
            );
            migrations::apply(connection, migration).map_err(|source| OpenError::Migration {
                version: migration.version,
                source,
                backup: backup.clone(),
            })?;
        }
        Ok(())
    }

    /// Fichier du catalogue.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Version de structure du catalogue (toujours la plus récente connue,
    /// puisque les migrations sont appliquées à l'ouverture).
    #[must_use]
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Réintègre dans le fichier du catalogue les modifications encore
    /// conservées dans le journal WAL (`Catalogue.sqlite-wal`), et vide ce
    /// journal.
    ///
    /// À appeler à la fermeture de l'application : le fichier du catalogue est
    /// alors complet à lui seul, et peut être copié sans ses fichiers annexes.
    ///
    /// # Errors
    ///
    /// Renvoie l'erreur SQLite si l'opération échoue ; les données restent
    /// alors intactes dans le journal et seront reprises à la prochaine
    /// ouverture.
    pub fn checkpoint(&self) -> Result<(), CatalogError> {
        self.connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(())
    }

    /// Connexion à la base, pour les fonctions de [`repositories`].
    #[must_use]
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Démarre une transaction : les modifications faites à travers elle sont
    /// toutes enregistrées par `commit()`, ou toutes annulées si elle est
    /// abandonnée (par exemple en cas d'erreur).
    ///
    /// # Errors
    ///
    /// Renvoie l'erreur SQLite si la transaction ne peut pas démarrer.
    pub fn transaction(&mut self) -> Result<Transaction<'_>, CatalogError> {
        Ok(self.connection.transaction()?)
    }
}

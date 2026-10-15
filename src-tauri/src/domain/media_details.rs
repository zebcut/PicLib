//! Informations techniques d'une photo ou d'une vidéo, lues dans ses
//! métadonnées.

/// Orientation EXIF (1 à 8) : comment faire pivoter ou retourner l'image
/// pour l'afficher à l'endroit. 1 = normale, 6 = pivotée de 90° dans le sens
/// horaire, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Orientation(u8);

impl Orientation {
    /// Orientation valide (1 à 8), sinon `None`.
    #[must_use]
    pub fn new(value: u8) -> Option<Self> {
        (1..=8).contains(&value).then_some(Self(value))
    }

    /// Valeur EXIF.
    #[must_use]
    pub fn get(self) -> u8 {
        self.0
    }
}

/// Position GPS en degrés décimaux (négatifs au sud et à l'ouest).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpsPosition {
    /// Latitude, de -90 à 90.
    pub latitude: f64,
    /// Longitude, de -180 à 180.
    pub longitude: f64,
    /// Altitude en mètres, si connue.
    pub altitude: Option<f64>,
}

/// Informations techniques d'un média. Tous les champs sont facultatifs :
/// un fichier peut n'en contenir aucune.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaDetails {
    /// Largeur en pixels, avant application de l'orientation.
    pub width: Option<u32>,
    /// Hauteur en pixels, avant application de l'orientation.
    pub height: Option<u32>,
    /// Orientation EXIF.
    pub orientation: Option<Orientation>,
    /// Marque de l'appareil (« Canon »...).
    pub camera_make: Option<String>,
    /// Modèle de l'appareil (« Canon EOS R6 »...).
    pub camera_model: Option<String>,
    /// Objectif (« RF70-200mm F2.8 L IS USM »...).
    pub lens: Option<String>,
    /// Focale en millimètres.
    pub focal_length_mm: Option<f64>,
    /// Ouverture (nombre f : 2,8 pour f/2.8).
    pub aperture: Option<f64>,
    /// Temps de pose en secondes (0,004 pour 1/250 s).
    pub exposure_time_s: Option<f64>,
    /// Sensibilité ISO.
    pub iso: Option<u32>,
    /// Vrai si le flash s'est déclenché.
    pub flash_fired: Option<bool>,
    /// Position GPS.
    pub gps: Option<GpsPosition>,
    /// Durée d'une vidéo, en millisecondes.
    pub duration_ms: Option<u64>,
}

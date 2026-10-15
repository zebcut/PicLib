//! Espaces de noms des propriétés XMP lues et écrites par PicLib. Seul
//! l'espace de noms compte : le préfixe varie d'un logiciel à l'autre.

/// RDF, la syntaxe du XMP.
pub(super) const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
/// Réservé au préfixe `xml` (`xml:lang`).
pub(super) const XML: &str = "http://www.w3.org/XML/1998/namespace";
/// Propriétés de base (`xmp:Rating`, `xmp:Label`).
pub(super) const XMP: &str = "http://ns.adobe.com/xap/1.0/";
/// Dublin Core (`dc:title`, `dc:subject`).
pub(super) const DC: &str = "http://purl.org/dc/elements/1.1/";
/// Photoshop (`photoshop:City`).
pub(super) const PHOTOSHOP: &str = "http://ns.adobe.com/photoshop/1.0/";
/// Lightroom (`lr:hierarchicalSubject`).
pub(super) const LIGHTROOM: &str = "http://ns.adobe.com/lightroom/1.0/";
/// TIFF (`tiff:Orientation`).
pub(super) const TIFF: &str = "http://ns.adobe.com/tiff/1.0/";
/// EXIF (`exif:GPSLatitude`).
pub(super) const EXIF: &str = "http://ns.adobe.com/exif/1.0/";
/// IPTC Core (`Iptc4xmpCore:Location`).
pub(super) const IPTC_CORE: &str = "http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/";
/// Propriétés propres à PicLib (`piclib:Pick`, le marqueur, qu'aucun champ
/// standard ne décrit).
pub(super) const PICLIB: &str = "http://ns.piclib.app/xmp/1.0/";

/// Préfixe habituel de chaque espace de noms écrit par PicLib.
pub(super) const PREFIXES: [(&str, &str); 8] = [
    (XMP, "xmp"),
    (DC, "dc"),
    (PHOTOSHOP, "photoshop"),
    (LIGHTROOM, "lr"),
    (TIFF, "tiff"),
    (EXIF, "exif"),
    (IPTC_CORE, "Iptc4xmpCore"),
    (PICLIB, "piclib"),
];

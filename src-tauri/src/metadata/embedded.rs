//! Métadonnées intégrées à une photo en plus de l'EXIF : paquet XMP et bloc
//! IPTC, repérés en un seul passage, **en lecture seule** (seuls les octets
//! nécessaires sont lus).
//!
//! - JPEG : segments APP1 (XMP) et APP13 (ressources Photoshop, dont
//!   l'IPTC), avant les données de l'image ;
//! - TIFF et RAW construits comme des TIFF (NEF, ARW, CR2, DNG) : balises
//!   700 (XMP), 33723 (IPTC) et 34377 (ressources Photoshop) du premier
//!   répertoire ;
//! - autres formats (HEIC, CR3, RAF, PNG) : recherche du paquet XMP dans le
//!   début du fichier, comme le prévoit la norme XMP pour les formats
//!   inconnus.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use super::recovered::RecoveredMetadata;
use super::{iptc, xmp};
use crate::domain::{MediaFormat, MediaKind};

/// Taille maximale d'un bloc lu. Les plus gros paquets XMP observés (avec
/// l'historique de Photoshop) font quelques centaines de Kio.
const MAX_BLOCK_SIZE: usize = 2 * 1024 * 1024;

/// En-tête d'un segment APP1 JPEG qui contient du XMP.
const JPEG_XMP_HEADER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

/// En-tête d'un segment APP1 JPEG qui contient de l'EXIF.
const JPEG_EXIF_HEADER: &[u8] = b"Exif\0\0";

/// En-tête d'un segment APP13 JPEG qui contient des ressources Photoshop.
const JPEG_PHOTOSHOP_HEADER: &[u8] = b"Photoshop 3.0\0";

/// Balises TIFF des blocs recherchés.
const TIFF_XMP_TAG: u16 = 700;
const TIFF_IPTC_TAG: u16 = 33723;
const TIFF_PHOTOSHOP_TAG: u16 = 34377;

/// Ressource Photoshop qui contient le bloc IPTC.
const PHOTOSHOP_IPTC_RESOURCE: u16 = 0x0404;

/// Quantité lue au début d'un fichier d'un autre format pour y chercher le
/// paquet : il y est placé tôt par tous les formats de nos fichiers de test.
const SCAN_SIZE: u64 = 256 * 1024;

const PACKET_START: &[u8] = b"<x:xmpmeta";
const PACKET_END: &[u8] = b"</x:xmpmeta>";

/// Métadonnées intégrées à une photo, en plus de l'EXIF.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EmbeddedMetadata {
    /// Contenu du paquet XMP.
    pub xmp: Option<RecoveredMetadata>,
    /// Contenu du bloc IPTC.
    pub iptc: Option<RecoveredMetadata>,
    /// Blocs illisibles ou démesurés, pour le journal : les autres servent
    /// quand même.
    pub problems: Vec<String>,
}

/// Lit le XMP et l'IPTC intégrés à une photo (rien pour une vidéo : leurs
/// métadonnées intégrées ne sont pas lues).
///
/// # Errors
///
/// Échoue si le fichier ne peut pas être lu.
pub fn read_embedded(path: &Path, format: MediaFormat) -> std::io::Result<EmbeddedMetadata> {
    if format.kind() != MediaKind::Photo {
        return Ok(EmbeddedMetadata::default());
    }
    let blocks = find_blocks(&mut BufReader::new(File::open(path)?), format)?;
    let mut embedded = EmbeddedMetadata {
        problems: blocks.problems,
        ..EmbeddedMetadata::default()
    };
    if let Some(packet) = blocks.xmp {
        match xmp::parse_packet(&packet) {
            Ok(xmp) => embedded.xmp = Some(xmp.metadata),
            Err(error) => embedded.problems.push(format!("embedded XMP: {error}")),
        }
    }
    if let Some(block) = blocks.iptc {
        match iptc::extract(&block) {
            Ok(iptc) => embedded.iptc = Some(iptc),
            Err(error) => embedded.problems.push(format!("IPTC: {error}")),
        }
    }
    Ok(embedded)
}

/// Segment EXIF d'un JPEG, trouvé par notre propre parcours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JpegExif {
    /// Contenu du segment APP1 « Exif », en-tête compris.
    Found(Vec<u8>),
    /// Le JPEG, bien formé jusqu'aux données de l'image, n'a pas d'EXIF.
    Absent,
    /// Le JPEG est mal formé : impossible de conclure.
    Unknown,
}

/// Cherche le segment EXIF d'un JPEG. Secours de `nom-exif` (version 3.8),
/// qui échoue quand un autre segment APP1 (XMP) précède l'EXIF ou le
/// remplace.
///
/// # Errors
///
/// Échoue si le fichier ne peut pas être lu.
pub fn jpeg_exif(path: &Path) -> std::io::Result<JpegExif> {
    let blocks = jpeg_blocks(&mut BufReader::new(File::open(path)?))?;
    Ok(match blocks.exif {
        Some(segment) => JpegExif::Found(segment),
        None if blocks.is_complete => JpegExif::Absent,
        None => JpegExif::Unknown,
    })
}

/// Octets des blocs trouvés.
#[derive(Debug, Default, PartialEq, Eq)]
struct Blocks {
    xmp: Option<Vec<u8>>,
    iptc: Option<Vec<u8>>,
    /// Segment EXIF d'un JPEG, en-tête compris.
    exif: Option<Vec<u8>>,
    /// Vrai si le parcours d'un JPEG est allé jusqu'aux données de l'image.
    is_complete: bool,
    problems: Vec<String>,
}

fn find_blocks(file: &mut (impl Read + Seek), format: MediaFormat) -> std::io::Result<Blocks> {
    match format {
        MediaFormat::Jpeg => jpeg_blocks(file),
        MediaFormat::Tiff
        | MediaFormat::Nef
        | MediaFormat::Arw
        | MediaFormat::Cr2
        | MediaFormat::Dng => tiff_blocks(file),
        _ => Ok(Blocks {
            xmp: scanned_packet(file)?,
            ..Blocks::default()
        }),
    }
}

/// Parcourt les segments d'un JPEG jusqu'au début des données de l'image.
fn jpeg_blocks(file: &mut (impl Read + Seek)) -> std::io::Result<Blocks> {
    let mut blocks = Blocks::default();
    let mut marker = [0_u8; 2];
    if read_exact_or_end(file, &mut marker)?.is_none() || marker != [0xFF, 0xD8] {
        return Ok(blocks);
    }
    loop {
        if read_exact_or_end(file, &mut marker)?.is_none() || marker[0] != 0xFF {
            return Ok(blocks);
        }
        match marker[1] {
            // Octet de remplissage : le vrai marqueur suit.
            0xFF => {
                file.seek(SeekFrom::Current(-1))?;
                continue;
            }
            // Début des données de l'image, ou fin : plus de métadonnées.
            0xDA | 0xD9 => {
                blocks.is_complete = true;
                return Ok(blocks);
            }
            // Marqueurs sans longueur.
            0x01 | 0xD0..=0xD7 => continue,
            _ => {}
        }
        let mut length = [0_u8; 2];
        if read_exact_or_end(file, &mut length)?.is_none() {
            return Ok(blocks);
        }
        let Some(content_length) = u16::from_be_bytes(length).checked_sub(2) else {
            return Ok(blocks);
        };
        let wanted = match marker[1] {
            0xE1 => blocks.xmp.is_none() || blocks.exif.is_none(),
            0xED => blocks.iptc.is_none(),
            _ => false,
        };
        if !wanted {
            file.seek(SeekFrom::Current(i64::from(content_length)))?;
            continue;
        }
        let mut content = vec![0_u8; usize::from(content_length)];
        if read_exact_or_end(file, &mut content)?.is_none() {
            return Ok(blocks);
        }
        if let Some(packet) = content.strip_prefix(JPEG_XMP_HEADER) {
            blocks.xmp.get_or_insert_with(|| packet.to_vec());
        } else if content.starts_with(JPEG_EXIF_HEADER) {
            blocks.exif.get_or_insert(content);
        } else if let Some(resources) = content.strip_prefix(JPEG_PHOTOSHOP_HEADER) {
            blocks.iptc = photoshop_iptc(resources).map(<[u8]>::to_vec);
        }
    }
}

/// Lit les balises utiles du premier répertoire (IFD0) d'un fichier TIFF.
fn tiff_blocks(file: &mut (impl Read + Seek)) -> std::io::Result<Blocks> {
    let mut blocks = Blocks::default();
    let mut header = [0_u8; 8];
    if read_exact_or_end(file, &mut header)?.is_none() {
        return Ok(blocks);
    }
    let order = match &header[..4] {
        b"II*\0" => ByteOrder::Little,
        b"MM\0*" => ByteOrder::Big,
        _ => return Ok(blocks),
    };
    file.seek(SeekFrom::Start(u64::from(order.u32(&header[4..8]))))?;
    let mut count = [0_u8; 2];
    if read_exact_or_end(file, &mut count)?.is_none() {
        return Ok(blocks);
    }
    let mut wanted = Vec::new();
    for _ in 0..order.u16(&count) {
        let mut entry = [0_u8; 12];
        if read_exact_or_end(file, &mut entry)?.is_none() {
            break;
        }
        let tag = order.u16(&entry[0..2]);
        if matches!(tag, TIFF_XMP_TAG | TIFF_IPTC_TAG | TIFF_PHOTOSHOP_TAG) {
            wanted.push((tag, entry));
        }
    }

    for (tag, entry) in wanted {
        // BYTE (1) ou UNDEFINED (7) : un octet par élément ; certains
        // logiciels écrivent l'IPTC en LONG (4), quatre octets par élément.
        let unit = match order.u16(&entry[2..4]) {
            1 | 7 => 1,
            4 => 4,
            _ => continue,
        };
        let length = usize::try_from(order.u32(&entry[4..8]))
            .unwrap_or(usize::MAX)
            .saturating_mul(unit);
        if length <= 4 {
            continue;
        }
        if length > MAX_BLOCK_SIZE {
            blocks.problems.push(format!("TIFF tag {tag} too large"));
            continue;
        }
        file.seek(SeekFrom::Start(u64::from(order.u32(&entry[8..12]))))?;
        let mut data = vec![0_u8; length];
        if read_exact_or_end(file, &mut data)?.is_none() {
            blocks.problems.push(format!("TIFF tag {tag} truncated"));
            continue;
        }
        match tag {
            TIFF_XMP_TAG => blocks.xmp = Some(data),
            TIFF_IPTC_TAG => blocks.iptc = Some(data),
            _ => {
                if blocks.iptc.is_none() {
                    blocks.iptc = photoshop_iptc(&data).map(<[u8]>::to_vec);
                }
            }
        }
    }
    Ok(blocks)
}

/// Bloc IPTC parmi des ressources Photoshop (`8BIM`, numéro, nom, taille,
/// données ; nom et données complétés à un nombre pair d'octets).
fn photoshop_iptc(mut resources: &[u8]) -> Option<&[u8]> {
    while let [b'8', b'B', b'I', b'M', high, low, name_length, rest @ ..] = resources {
        let id = u16::from_be_bytes([*high, *low]);
        // Longueur du nom, octet de longueur compris, arrondie au pair.
        let name_size = (usize::from(*name_length) + 1).next_multiple_of(2);
        let rest = rest.get(name_size - 1..)?;
        let size = usize::try_from(u32::from_be_bytes(rest.get(..4)?.try_into().ok()?)).ok()?;
        let data = rest.get(4..4 + size)?;
        if id == PHOTOSHOP_IPTC_RESOURCE {
            return Some(data);
        }
        resources = rest.get(4 + size.next_multiple_of(2)..)?;
    }
    None
}

/// Cherche le paquet XMP dans le début du fichier.
fn scanned_packet(file: &mut impl Read) -> std::io::Result<Option<Vec<u8>>> {
    let mut prefix = Vec::new();
    file.take(SCAN_SIZE).read_to_end(&mut prefix)?;
    let Some(start) = find(&prefix, PACKET_START) else {
        return Ok(None);
    };
    let Some(length) = find(&prefix[start..], PACKET_END) else {
        return Ok(None);
    };
    Ok(Some(
        prefix[start..start + length + PACKET_END.len()].to_vec(),
    ))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Remplit `buffer` ; `None` si le fichier se termine avant.
fn read_exact_or_end(file: &mut impl Read, buffer: &mut [u8]) -> std::io::Result<Option<()>> {
    match file.read_exact(buffer) {
        Ok(()) => Ok(Some(())),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
        Err(error) => Err(error),
    }
}

/// Ordre des octets d'un fichier TIFF.
#[derive(Debug, Clone, Copy)]
enum ByteOrder {
    Little,
    Big,
}

impl ByteOrder {
    fn u16(self, bytes: &[u8]) -> u16 {
        let bytes = [bytes[0], bytes[1]];
        match self {
            Self::Little => u16::from_le_bytes(bytes),
            Self::Big => u16::from_be_bytes(bytes),
        }
    }

    fn u32(self, bytes: &[u8]) -> u32 {
        let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
        match self {
            Self::Little => u32::from_le_bytes(bytes),
            Self::Big => u32::from_be_bytes(bytes),
        }
    }
}

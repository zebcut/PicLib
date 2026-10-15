//! Lecture des champs IPTC « IIM », l'ancien format de métadonnées de la
//! presse, encore écrit par beaucoup de logiciels dans les JPEG et TIFF
//!.
//!
//! Le bloc est une suite d'enregistrements : l'octet 0x1C, le numéro
//! d'enregistrement, le numéro du champ, la longueur, puis la valeur.

use jiff::civil::DateTime;

use super::recovered::{KeywordPath, RecoveredMetadata, field_value};
use super::values::parse_offset;
use crate::domain::{CaptureTime, MetadataField, recovered_keyword_name};

/// Octet qui ouvre chaque enregistrement.
const TAG_MARKER: u8 = 0x1C;

/// Séquence qui annonce des textes en UTF-8 (champ 1:90).
const UTF8_DECLARATION: &[u8] = b"\x1b%G";

/// Séparateur de plusieurs auteurs réunis dans un seul champ.
const CREATOR_SEPARATOR: &str = "; ";

/// Bloc IPTC illisible.
#[derive(Debug, thiserror::Error)]
#[error("malformed IPTC: {0}")]
pub struct IptcError(&'static str);

/// Champs IPTC utiles, avec leur numéro dans l'enregistrement 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Dataset {
    ObjectName = 5,
    Keywords = 25,
    DateCreated = 55,
    TimeCreated = 60,
    ByLine = 80,
    City = 90,
    SubLocation = 92,
    ProvinceState = 95,
    Country = 101,
    CopyrightNotice = 116,
    Caption = 120,
}

/// Lit les informations utiles d'un bloc IPTC.
///
/// # Errors
///
/// Échoue si le bloc est tronqué ou mal formé.
pub fn extract(bytes: &[u8]) -> Result<RecoveredMetadata, IptcError> {
    let records = parse(bytes)?;
    let declared_utf8 = records
        .iter()
        .any(|record| record.number == (1, 90) && record.value == UTF8_DECLARATION);
    let texts = |dataset: Dataset| -> Vec<String> {
        records
            .iter()
            .filter(|record| record.number == (2, dataset as u8))
            .map(|record| decode(record.value, declared_utf8))
            .collect()
    };
    let text = |dataset: Dataset| texts(dataset).into_iter().next();

    let creators: Vec<String> = texts(Dataset::ByLine)
        .into_iter()
        .map(|creator| creator.trim().to_owned())
        .filter(|creator| !creator.is_empty())
        .collect();
    let fields = [
        (MetadataField::Title, text(Dataset::ObjectName)),
        (MetadataField::Caption, text(Dataset::Caption)),
        (
            MetadataField::Creator,
            Some(creators.join(CREATOR_SEPARATOR)),
        ),
        (MetadataField::Copyright, text(Dataset::CopyrightNotice)),
        (MetadataField::Country, text(Dataset::Country)),
        (MetadataField::Region, text(Dataset::ProvinceState)),
        (MetadataField::City, text(Dataset::City)),
        (MetadataField::Location, text(Dataset::SubLocation)),
    ]
    .into_iter()
    .filter_map(|(field, value)| Some((field, field_value(field, &value?)?)))
    .collect();

    let mut keywords: Vec<KeywordPath> = Vec::new();
    for name in texts(Dataset::Keywords)
        .iter()
        .filter_map(|name| recovered_keyword_name(name))
    {
        let path = KeywordPath::Name(name);
        if !keywords.contains(&path) {
            keywords.push(path);
        }
    }

    Ok(RecoveredMetadata {
        fields,
        capture_time: text(Dataset::DateCreated)
            .and_then(|date| capture_time(&date, text(Dataset::TimeCreated).as_deref())),
        keywords,
        ..RecoveredMetadata::default()
    })
}

/// Un enregistrement : numéro (enregistrement, champ) et valeur brute.
struct Record<'a> {
    number: (u8, u8),
    value: &'a [u8],
}

fn parse(mut bytes: &[u8]) -> Result<Vec<Record<'_>>, IptcError> {
    let mut records = Vec::new();
    while let [TAG_MARKER, record, dataset, high, low, rest @ ..] = bytes {
        let length = u16::from_be_bytes([*high, *low]);
        let (length, rest) = if length & 0x8000 == 0 {
            (usize::from(length), rest)
        } else {
            // Longueur « étendue » : les 15 bits suivants donnent le nombre
            // d'octets de la vraie longueur.
            let size = usize::from(length & 0x7FFF);
            if size == 0 || size > 4 || rest.len() < size {
                return Err(IptcError("invalid extended length"));
            }
            let (length, rest) = rest.split_at(size);
            let length = length
                .iter()
                .fold(0_usize, |total, byte| (total << 8) | usize::from(*byte));
            (length, rest)
        };
        if rest.len() < length {
            return Err(IptcError("truncated record"));
        }
        let (value, rest) = rest.split_at(length);
        records.push(Record {
            number: (*record, *dataset),
            value,
        });
        bytes = rest;
    }
    // Des octets nuls complètent souvent le bloc ; autre chose est suspect.
    if bytes.iter().any(|byte| *byte != 0) {
        return Err(IptcError("unexpected data"));
    }
    Ok(records)
}

/// Texte d'un champ : UTF-8 s'il est annoncé ou valide, sinon Windows-1252,
/// l'encodage des anciens logiciels (une table suffit : un octet, un
/// caractère).
fn decode(bytes: &[u8], declared_utf8: bool) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) if declared_utf8 => String::from_utf8_lossy(bytes).into_owned(),
        Err(_) => bytes.iter().map(|&byte| windows_1252(byte)).collect(),
    }
}

/// Caractère Windows-1252 d'un octet : Latin-1, sauf 0x80 à 0x9F.
fn windows_1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    match byte {
        0x80..=0x9F => HIGH[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

/// Date IPTC : `AAAAMMJJ` (2:55), heure `HHMMSS±HHMM` (2:60) facultative.
fn capture_time(date: &str, time: Option<&str>) -> Option<CaptureTime> {
    let date = date.trim();
    if date.len() != 8 {
        return None;
    }
    let time = time.map_or("", str::trim);
    let (clock, offset) = match time.get(6..) {
        Some(zone) if !zone.is_empty() => {
            let offset = format!("{}:{}", zone.get(..3)?, zone.get(3..)?);
            (&time[..6], Some(parse_offset(&offset)?))
        }
        _ => (time, None),
    };
    let (hour, minute, second) = if clock.is_empty() {
        (0, 0, 0)
    } else {
        (
            number(clock, 0..2)?,
            number(clock, 2..4)?,
            number(clock, 4..6)?,
        )
    };
    let local = DateTime::new(
        number(date, 0..4)?,
        number(date, 4..6)?,
        number(date, 6..8)?,
        hour,
        minute,
        second,
        0,
    )
    .ok()?;
    Some(CaptureTime::from_exif(local, offset))
}

fn number<T: std::str::FromStr>(text: &str, range: std::ops::Range<usize>) -> Option<T> {
    text.get(range)?.parse().ok()
}

//! Conversion des valeurs brutes de `nom-exif` vers nos types.

use chrono::{Datelike, Timelike};
use jiff::civil::DateTime;
use jiff::tz::{Offset, TimeZone};
use nom_exif::{EntryValue, ExifDateTime, GPSInfo};

use crate::domain::{CaptureTime, GpsPosition};

/// Texte non vide, sans espaces superflus (les appareils complètent souvent
/// leurs champs avec des espaces ou des caractères nuls).
pub fn text(value: Option<&EntryValue>) -> Option<String> {
    let text = value?
        .as_str()?
        .trim_matches(|c: char| c.is_whitespace() || c == '\0');
    (!text.is_empty()).then(|| text.to_owned())
}

/// Entier positif, quelle que soit sa taille de stockage ; pour une liste
/// (certains appareils écrivent l'ISO ainsi), le premier élément.
pub fn unsigned(value: Option<&EntryValue>) -> Option<u64> {
    match value? {
        EntryValue::U8(v) => Some(u64::from(*v)),
        EntryValue::U16(v) => Some(u64::from(*v)),
        EntryValue::U32(v) => Some(u64::from(*v)),
        EntryValue::U64(v) => Some(*v),
        EntryValue::U16Array(values) => values.first().map(|v| u64::from(*v)),
        EntryValue::U32Array(values) => values.first().map(|v| u64::from(*v)),
        _ => None,
    }
}

/// Image or video dimension in pixels. Some software writes 0 when it
/// re-saves a file: a zero is treated as missing (the catalog only stores
/// positive sizes; the real one is read when the image is decoded).
pub fn pixel_size(value: Option<&EntryValue>) -> Option<u32> {
    unsigned(value)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|&size| size > 0)
}

/// Nombre décimal, depuis une fraction EXIF ou un nombre ; `None` pour une
/// valeur non finie ou nulle au dénominateur.
pub fn decimal(value: Option<&EntryValue>) -> Option<f64> {
    let number = match value? {
        EntryValue::URational(rational) => rational.to_f64()?,
        EntryValue::IRational(rational) => rational.to_f64()?,
        EntryValue::URationalArray(values) => values.first()?.to_f64()?,
        EntryValue::F32(v) => f64::from(*v),
        EntryValue::F64(v) => *v,
        other => unsigned(Some(other))
            .and_then(|v| u32::try_from(v).ok())
            .map(f64::from)?,
    };
    number.is_finite().then_some(number)
}

/// Date de prise de vue. `offset_text` (`OffsetTimeOriginal`, « +02:00 »)
/// complète une date sans fuseau quand l'appareil l'a écrit à part.
pub fn capture_time(
    value: Option<&EntryValue>,
    offset_text: Option<&EntryValue>,
) -> Option<CaptureTime> {
    match value?.as_datetime()? {
        ExifDateTime::Aware(aware) => {
            let offset = Offset::from_seconds(aware.offset().local_minus_utc()).ok();
            Some(CaptureTime::from_exif(civil(&aware.naive_local())?, offset))
        }
        ExifDateTime::Naive(naive) => {
            let offset = offset_text.and_then(|text| parse_offset(text.as_str()?));
            Some(CaptureTime::from_exif(civil(&naive)?, offset))
        }
    }
}

/// Date de création d'une vidéo. Les conteneurs MP4/MOV l'enregistrent en
/// heure universelle (UTC) : elle est alors exprimée dans `time_zone` (celui
/// du Mac), comme le ferait l'appareil. Une date écrite avec un autre
/// décalage (iPhone, par exemple) est gardée telle quelle.
pub fn video_capture_time(value: Option<&EntryValue>, time_zone: &TimeZone) -> Option<CaptureTime> {
    match value?.as_datetime()? {
        ExifDateTime::Aware(aware) if aware.offset().local_minus_utc() == 0 => {
            let instant = jiff::Timestamp::from_second(aware.timestamp()).ok()?;
            Some(CaptureTime::from_exif(
                time_zone.to_datetime(instant),
                Some(time_zone.to_offset(instant)),
            ))
        }
        _ => capture_time(value, None),
    }
}

/// Position GPS, si elle est complète et plausible.
pub fn gps_position(gps: &GPSInfo) -> Option<GpsPosition> {
    let latitude = gps.latitude_decimal()?;
    let longitude = gps.longitude_decimal()?;
    is_plausible_position(latitude, longitude).then(|| GpsPosition {
        latitude,
        longitude,
        altitude: gps.altitude_meters().filter(|meters| meters.is_finite()),
    })
}

/// Vrai pour une position possible. Exactement 0, 0 est écarté : c'est ce
/// qu'enregistre un appareil qui n'a pas trouvé sa position, et ce point est
/// en mer (golfe de Guinée), où personne ne photographie à 1 cm près.
pub fn is_plausible_position(latitude: f64, longitude: f64) -> bool {
    let in_range = (-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude);
    // Comparaison exacte voulue : seul le « zéro » d'un GPS sans position
    // est écarté, pas un point réel proche.
    #[allow(clippy::float_cmp)]
    let is_null_island = latitude == 0.0 && longitude == 0.0;
    in_range && !is_null_island
}

/// Date ISO 8601 (XMP, `.dop` de DxO), plus ou moins précise : `2024`, `2024-07`,
/// `2024-07-14`, `2024-07-14T18:30`, `2024-07-14T18:30:05.12+02:00`... Ce
/// qui manque vaut le début de la période (1er janvier, minuit).
pub fn parse_iso_date(text: &str) -> Option<CaptureTime> {
    let (date, time) = text.split_once('T').unwrap_or((text, ""));
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse().ok()?;
    let month = date_parts
        .next()
        .map_or(Some(1), |part| part.parse().ok())?;
    let day = date_parts
        .next()
        .map_or(Some(1), |part| part.parse().ok())?;
    if date_parts.next().is_some() {
        return None;
    }

    let (clock, offset) = split_offset(time)?;
    let mut clock_parts = clock.split(':');
    let mut next = || {
        clock_parts
            .next()
            .filter(|part| !part.is_empty())
            .map_or(Some(0.0), |part| part.parse::<f64>().ok())
    };
    let (hour, minute, second) = (next()?, next()?, next()?);
    // Conversions sûres : `DateTime::new` refuse ensuite toute valeur hors
    // des bornes d'une heure.
    #[allow(clippy::cast_possible_truncation)]
    let local = DateTime::new(
        year,
        month,
        day,
        hour as i8,
        minute as i8,
        second.trunc() as i8,
        0,
    )
    .ok()?;
    Some(CaptureTime::from_exif(local, offset))
}

/// Sépare l'heure de son décalage (`Z`, `+02:00`, `-04:30`).
fn split_offset(time: &str) -> Option<(&str, Option<Offset>)> {
    if let Some(clock) = time.strip_suffix('Z') {
        return Some((clock, Some(Offset::UTC)));
    }
    match time.rfind(['+', '-']) {
        Some(index) => Some((&time[..index], Some(parse_offset(&time[index..])?))),
        None => Some((time, None)),
    }
}

fn civil(naive: &chrono::NaiveDateTime) -> Option<DateTime> {
    DateTime::new(
        i16::try_from(naive.year()).ok()?,
        i8::try_from(naive.month()).ok()?,
        i8::try_from(naive.day()).ok()?,
        i8::try_from(naive.hour()).ok()?,
        i8::try_from(naive.minute()).ok()?,
        i8::try_from(naive.second()).ok()?,
        0,
    )
    .ok()
}

/// Décalage EXIF (« +02:00 », « -04:30 ») en décalage jiff.
pub fn parse_offset(text: &str) -> Option<Offset> {
    let (sign, rest) = match text.trim().split_at_checked(1)? {
        ("+", rest) => (1, rest),
        ("-", rest) => (-1, rest),
        _ => return None,
    };
    let (hours, minutes) = rest.split_once(':')?;
    let seconds = sign * (hours.parse::<i32>().ok()? * 3600 + minutes.parse::<i32>().ok()? * 60);
    Offset::from_seconds(seconds).ok()
}

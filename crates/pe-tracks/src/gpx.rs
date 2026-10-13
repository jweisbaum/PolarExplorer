//! GPX track files (GPX 1.0 and 1.1): track points and their UTC times,
//! with optional course and speed fields.

use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};

use pe_core::track::Fix;

use crate::error::{Reason, Result, TrackFileError};
use crate::time::{TimeFormat, parse_time};
use crate::{MAX_FIXES, RawTrack, check_heading, check_speed, position};

const MPS_TO_KNOTS: f64 = 1.943_844_492_440_6;

#[derive(Default)]
struct Point {
    lat: Option<f64>,
    lon: Option<f64>,
    time: Option<String>,
    course: Option<f64>,
    speed: Option<f64>,
}

#[derive(Default)]
struct TrackBuilder {
    name: Option<String>,
    fixes: Vec<Fix>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    None,
    Name,
    Time,
    Course,
    Speed,
}

fn bad(message: impl Into<String>) -> TrackFileError {
    TrackFileError::whole(Reason::BadGpx(message.into()))
}

fn parse_number(text: &str) -> Result<f64> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .ok_or_else(|| TrackFileError::whole(Reason::NotANumber(text.to_owned())))
}

/// Reads GPX track points as named tracks, preserving point order for the
/// normal track builder to sort and report. GPX speed values are metres per
/// second and are converted to the project's knot convention.
pub fn read_gpx(bytes: &[u8]) -> Result<Vec<RawTrack>> {
    let text = std::str::from_utf8(bytes).map_err(|e| bad(format!("not UTF-8: {e}")))?;
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    let mut current: Option<TrackBuilder> = None;
    let mut point: Option<Point> = None;
    let mut field = Field::None;
    let mut value = String::new();
    let mut tracks = Vec::new();
    let mut fixes = 0usize;

    loop {
        let event = reader.read_event().map_err(|e| bad(e.to_string()))?;
        match event {
            Event::Start(element) => match element.local_name().as_ref() {
                "trk" => current = Some(TrackBuilder::default()),
                "trkpt" if current.is_some() => {
                    let mut next = Point::default();
                    for attribute in element.attributes() {
                        let attribute = attribute.map_err(|e| bad(e.to_string()))?;
                        let key = attribute.key.local_name();
                        let decoded = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .map_err(|e| bad(e.to_string()))?;
                        match key.as_ref() {
                            "lat" => next.lat = Some(parse_number(&decoded)?),
                            "lon" => next.lon = Some(parse_number(&decoded)?),
                            _ => {}
                        }
                    }
                    point = Some(next);
                }
                "name" if current.is_some() && point.is_none() => {
                    field = Field::Name;
                    value.clear();
                }
                "time" if point.is_some() => {
                    field = Field::Time;
                    value.clear();
                }
                "course" if point.is_some() => {
                    field = Field::Course;
                    value.clear();
                }
                "speed" if point.is_some() => {
                    field = Field::Speed;
                    value.clear();
                }
                _ => {}
            },
            Event::Empty(element)
                if element.local_name().as_ref() == "trkpt" && current.is_some() =>
            {
                return Err(bad("a track point has no time"));
            }
            Event::Text(text) if field != Field::None => {
                value.push_str(&text.xml10_content());
            }
            Event::CData(text) if field != Field::None => {
                value.push_str(&text.xml10_content());
            }
            Event::End(element) => match element.local_name().as_ref() {
                "name" if field == Field::Name => {
                    if let Some(track) = current.as_mut() {
                        track.name = (!value.trim().is_empty()).then(|| value.trim().to_owned());
                    }
                    field = Field::None;
                }
                "time" if field == Field::Time => {
                    if let Some(point) = point.as_mut() {
                        point.time = Some(value.trim().to_owned());
                    }
                    field = Field::None;
                }
                "course" if field == Field::Course => {
                    if let Some(point) = point.as_mut() {
                        point.course = Some(parse_number(&value)?);
                    }
                    field = Field::None;
                }
                "speed" if field == Field::Speed => {
                    if let Some(point) = point.as_mut() {
                        point.speed = Some(parse_number(&value)?);
                    }
                    field = Field::None;
                }
                "trkpt" => {
                    let point = point
                        .take()
                        .ok_or_else(|| bad("a closing track point has no opening point"))?;
                    let (lat, lon) = position(
                        point
                            .lat
                            .ok_or_else(|| bad("a track point has no latitude"))?,
                        point
                            .lon
                            .ok_or_else(|| bad("a track point has no longitude"))?,
                    )
                    .ok_or_else(|| TrackFileError::whole(Reason::BadPosition))?;
                    let time = point
                        .time
                        .ok_or_else(|| TrackFileError::whole(Reason::NoTime))?;
                    let t = parse_time(&time, &TimeFormat::Iso8601)
                        .map_err(|error| TrackFileError::whole(Reason::BadTime(error)))?;
                    let cog = check_heading(point.course).map_err(TrackFileError::whole)?;
                    let sog = check_speed(point.speed.map(|speed| speed * MPS_TO_KNOTS))
                        .map_err(TrackFileError::whole)?;
                    current
                        .as_mut()
                        .ok_or_else(|| bad("a track point is outside a track"))?
                        .fixes
                        .push(Fix {
                            t,
                            lat,
                            lon,
                            cog,
                            sog,
                            tws: None,
                            twd_from: None,
                        });
                    fixes += 1;
                    if fixes > MAX_FIXES {
                        return Err(TrackFileError::whole(Reason::TooManyFixes));
                    }
                }
                "trk" => {
                    if let Some(track) = current.take()
                        && !track.fixes.is_empty()
                    {
                        tracks.push(RawTrack {
                            boat: track.name,
                            fixes: track.fixes,
                        });
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    if point.is_some() || current.is_some() {
        return Err(bad("the GPX track did not close"));
    }
    if tracks.is_empty() {
        return Err(TrackFileError::whole(Reason::NoFixes));
    }
    Ok(tracks)
}

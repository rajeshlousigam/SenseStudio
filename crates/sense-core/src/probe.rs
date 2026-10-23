use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atoms::{self, BoxLayout};
use crate::tools::{run_capture, Tools};
use crate::{Error, Result};

/// Seconds of packets sampled from the start to judge frame timing and GOP length.
const TIMING_SAMPLE_SECONDS: u32 = 30;
/// Shorter sample for links: every sampled second is fetched over the network.
const REMOTE_TIMING_SAMPLE_SECONDS: u32 = 10;

#[derive(Debug, Clone, Serialize)]
pub struct MediaInfo {
    pub path: PathBuf,
    pub format_name: String,
    pub duration: Option<f64>,
    pub size: Option<u64>,
    pub bit_rate: Option<u64>,
    /// Present for MOV/MP4 family files only.
    pub layout: Option<BoxLayout>,
    pub video: Option<VideoStream>,
    pub audio: Option<AudioStream>,
    /// Streams other than the first video and first audio (timecode, GPS, subtitles, extra tracks).
    pub extra_streams: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoStream {
    pub index: u32,
    pub codec: String,
    pub profile: Option<String>,
    pub level: Option<i32>,
    pub width: u32,
    pub height: u32,
    pub pix_fmt: Option<String>,
    pub color_transfer: Option<String>,
    pub color_primaries: Option<String>,
    /// Clockwise display rotation normalised to 0, 90, 180 or 270.
    pub rotation: u32,
    pub avg_fps: Option<f64>,
    pub timing: Option<FrameTiming>,
}

impl VideoStream {
    /// Size as shown to the viewer, after applying rotation.
    pub fn display_size(&self) -> (u32, u32) {
        if self.rotation % 180 == 90 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    pub fn is_hdr(&self) -> bool {
        matches!(self.color_transfer.as_deref(), Some("smpte2084" | "arib-std-b67"))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrameTiming {
    pub sampled_frames: usize,
    pub variable_frame_rate: bool,
    /// Longest keyframe distance seen in the sample. When the sample holds a
    /// single keyframe this is the sample span, i.e. a lower bound.
    pub max_keyframe_interval: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioStream {
    pub index: u32,
    pub codec: String,
    pub profile: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
}

impl MediaInfo {
    pub fn major_brand(&self) -> Option<&str> {
        self.layout.as_ref().and_then(|l| l.major_brand.as_deref())
    }

    pub fn is_mov_family(&self) -> bool {
        self.format_name.split(',').any(|f| f == "mov" || f == "mp4")
    }

    pub fn streams_summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(v) = &self.video {
            let fps = v.avg_fps.map(|f| format!(" {f:.3}fps")).unwrap_or_default();
            parts.push(format!("video:{} {}x{}{fps}", v.codec, v.width, v.height));
        }
        if let Some(a) = &self.audio {
            parts.push(format!("audio:{}", a.codec));
        }
        if self.extra_streams > 0 {
            parts.push(format!("+{} other", self.extra_streams));
        }
        parts.join("; ")
    }
}

pub fn probe(tools: &Tools, path: &Path) -> Result<MediaInfo> {
    probe_source(tools, path.as_os_str(), TIMING_SAMPLE_SECONDS, || {
        atoms::scan(&mut BufReader::new(File::open(path)?))
    })
}

/// Checks a video link without downloading it. ffprobe streams what it needs and the
/// box layout is read through `reader`, an HTTP range reader supplied by the caller.
pub fn probe_remote<R: Read + Seek>(tools: &Tools, url: &str, reader: &mut R) -> Result<MediaInfo> {
    probe_source(tools, OsStr::new(url), REMOTE_TIMING_SAMPLE_SECONDS, || atoms::scan(reader))
}

fn probe_source(
    tools: &Tools,
    input: &OsStr,
    sample_seconds: u32,
    scan_layout: impl FnOnce() -> std::io::Result<BoxLayout>,
) -> Result<MediaInfo> {
    let path = Path::new(input);
    let args: [&OsStr; 7] = [
        "-v".as_ref(),
        "error".as_ref(),
        "-show_format".as_ref(),
        "-show_streams".as_ref(),
        "-of".as_ref(),
        "json".as_ref(),
        path.as_os_str(),
    ];
    let raw: RawProbe = parse_json(&run_capture("ffprobe", &tools.ffprobe, &args)?, "ffprobe output")?;

    let streams = raw.streams.unwrap_or_default();
    if streams.is_empty() {
        return Err(Error::Tool { tool: "ffprobe", message: "no media streams found".into() });
    }
    let format = raw.format.unwrap_or_default();

    let video_raw = streams.iter().find(|s| s.codec_type.as_deref() == Some("video") && !s.is_attached_pic());
    let audio_raw = streams.iter().find(|s| s.codec_type.as_deref() == Some("audio"));
    let extra_streams = streams.len() - usize::from(video_raw.is_some()) - usize::from(audio_raw.is_some());

    let mut video = video_raw.map(RawStream::to_video);
    if let Some(v) = video.as_mut() {
        v.timing = Some(sample_timing(tools, path, sample_seconds)?);
    }

    let mut info = MediaInfo {
        path: path.to_owned(),
        format_name: format.format_name.unwrap_or_default(),
        duration: format.duration.as_deref().and_then(|d| d.parse().ok()),
        size: format.size.as_deref().and_then(|s| s.parse().ok()),
        bit_rate: format.bit_rate.as_deref().and_then(|s| s.parse().ok()),
        layout: None,
        video,
        audio: audio_raw.map(RawStream::to_audio),
        extra_streams,
    };
    if info.is_mov_family() {
        let mut layout = scan_layout()?;
        // ffprobe reports the brand even if our header walk could not reach ftyp.
        if layout.major_brand.is_none() {
            layout.major_brand = format.tags.and_then(|tags| {
                tags.into_iter().find(|(k, _)| k.eq_ignore_ascii_case("major_brand")).map(|(_, v)| v)
            });
        }
        info.layout = Some(layout);
    }
    Ok(info)
}

fn sample_timing(tools: &Tools, path: &Path, seconds: u32) -> Result<FrameTiming> {
    let interval = format!("%+{seconds}");
    let args: [&OsStr; 11] = [
        "-v".as_ref(),
        "error".as_ref(),
        "-select_streams".as_ref(),
        "v:0".as_ref(),
        "-read_intervals".as_ref(),
        interval.as_ref(),
        "-show_entries".as_ref(),
        "packet=pts_time,flags".as_ref(),
        "-of".as_ref(),
        "json".as_ref(),
        path.as_os_str(),
    ];
    let raw: RawPackets = parse_json(&run_capture("ffprobe", &tools.ffprobe, &args)?, "ffprobe packet list")?;

    let mut pts = Vec::new();
    let mut keyframes = Vec::new();
    for packet in raw.packets.unwrap_or_default() {
        let Some(t) = packet.pts_time.as_deref().and_then(|t| t.parse::<f64>().ok()) else { continue };
        pts.push(t);
        if packet.flags.as_deref().is_some_and(|f| f.starts_with('K')) {
            keyframes.push(t);
        }
    }
    Ok(analyze_timing(pts, keyframes))
}

/// Pure timing analysis over presentation timestamps (decode order is fine; they are sorted here).
pub fn analyze_timing(mut pts: Vec<f64>, mut keyframes: Vec<f64>) -> FrameTiming {
    pts.sort_by(f64::total_cmp);
    pts.dedup();
    keyframes.sort_by(f64::total_cmp);

    let deltas: Vec<f64> = pts.windows(2).map(|w| w[1] - w[0]).filter(|d| *d > 0.0).collect();
    let variable_frame_rate = if deltas.len() < 10 {
        false
    } else {
        let mut sorted = deltas.clone();
        sorted.sort_by(f64::total_cmp);
        let median = sorted[sorted.len() / 2];
        // 25% tolerance absorbs timebase rounding; a couple of dropped frames is not VFR.
        let outliers = deltas.iter().filter(|d| (*d - median).abs() > median * 0.25).count();
        outliers > 2.max(deltas.len() / 100)
    };

    let span = match (pts.first(), pts.last()) {
        (Some(first), Some(last)) => last - first,
        _ => 0.0,
    };
    let max_keyframe_interval = match keyframes.len() {
        0 => None,
        1 => Some(span),
        _ => keyframes.windows(2).map(|w| w[1] - w[0]).reduce(f64::max),
    };

    FrameTiming { sampled_frames: pts.len(), variable_frame_rate, max_keyframe_interval }
}

fn parse_json<T: for<'de> Deserialize<'de>>(text: &str, what: &'static str) -> Result<T> {
    serde_json::from_str(text).map_err(|e| Error::Parse { what, message: e.to_string() })
}

/// "30000/1001" -> 29.97; "0/0" and garbage -> None.
pub(crate) fn parse_rate(rate: &str) -> Option<f64> {
    let (num, den) = rate.split_once('/').unwrap_or((rate, "1"));
    let (num, den): (f64, f64) = (num.parse().ok()?, den.parse().ok()?);
    (num > 0.0 && den > 0.0).then(|| num / den)
}

#[derive(Deserialize)]
struct RawProbe {
    streams: Option<Vec<RawStream>>,
    format: Option<RawFormat>,
}

#[derive(Deserialize, Default)]
struct RawFormat {
    format_name: Option<String>,
    duration: Option<String>,
    size: Option<String>,
    bit_rate: Option<String>,
    tags: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct RawStream {
    index: u32,
    codec_type: Option<String>,
    codec_name: Option<String>,
    profile: Option<String>,
    level: Option<i32>,
    width: Option<u32>,
    height: Option<u32>,
    pix_fmt: Option<String>,
    color_transfer: Option<String>,
    color_primaries: Option<String>,
    avg_frame_rate: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
    disposition: Option<HashMap<String, i64>>,
    side_data_list: Option<Vec<RawSideData>>,
    tags: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct RawSideData {
    rotation: Option<f64>,
}

impl RawStream {
    fn is_attached_pic(&self) -> bool {
        self.disposition.as_ref().and_then(|d| d.get("attached_pic")).is_some_and(|v| *v == 1)
    }

    fn rotation(&self) -> u32 {
        let from_side_data = self.side_data_list.iter().flatten().find_map(|sd| sd.rotation);
        let from_tag = self.tags.as_ref().and_then(|t| t.get("rotate")).and_then(|r| r.parse::<f64>().ok());
        let degrees = from_side_data.or(from_tag).unwrap_or(0.0).round() as i64;
        // Only the axis matters for sizing; snap odd angles to the nearest quarter turn.
        (((degrees.rem_euclid(360) + 45) / 90 * 90) % 360) as u32
    }

    fn to_video(&self) -> VideoStream {
        VideoStream {
            index: self.index,
            codec: self.codec_name.clone().unwrap_or_default(),
            profile: self.profile.clone(),
            level: self.level.filter(|l| *l > 0),
            width: self.width.unwrap_or(0),
            height: self.height.unwrap_or(0),
            pix_fmt: self.pix_fmt.clone(),
            color_transfer: self.color_transfer.clone(),
            color_primaries: self.color_primaries.clone(),
            rotation: self.rotation(),
            avg_fps: self.avg_frame_rate.as_deref().and_then(parse_rate),
            timing: None,
        }
    }

    fn to_audio(&self) -> AudioStream {
        AudioStream {
            index: self.index,
            codec: self.codec_name.clone().unwrap_or_default(),
            profile: self.profile.clone(),
            sample_rate: self.sample_rate.as_deref().and_then(|s| s.parse().ok()),
            channels: self.channels,
        }
    }
}

#[derive(Deserialize)]
struct RawPackets {
    packets: Option<Vec<RawPacket>>,
}

#[derive(Deserialize)]
struct RawPacket {
    pts_time: Option<String>,
    flags: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(fps: f64, count: usize) -> Vec<f64> {
        (0..count).map(|i| i as f64 / fps).collect()
    }

    #[test]
    fn constant_rate_with_one_dropped_frame_is_cfr() {
        let mut pts = frames(30.0, 300);
        pts.remove(150);
        let timing = analyze_timing(pts, vec![0.0, 2.0, 4.0, 6.0, 8.0]);
        assert!(!timing.variable_frame_rate);
        assert_eq!(timing.max_keyframe_interval, Some(2.0));
    }

    #[test]
    fn irregular_gaps_are_vfr() {
        let pts: Vec<f64> = frames(30.0, 300).into_iter().enumerate().filter(|(i, _)| i % 10 < 6).map(|(_, t)| t).collect();
        assert!(analyze_timing(pts, vec![0.0]).variable_frame_rate);
    }

    #[test]
    fn single_keyframe_reports_span_as_lower_bound() {
        let timing = analyze_timing(frames(25.0, 251), vec![0.0]);
        assert_eq!(timing.max_keyframe_interval, Some(10.0));
    }

    #[test]
    fn parses_rates() {
        assert!((parse_rate("30000/1001").unwrap() - 29.97).abs() < 0.01);
        assert_eq!(parse_rate("0/0"), None);
        assert_eq!(parse_rate("25"), Some(25.0));
    }
}

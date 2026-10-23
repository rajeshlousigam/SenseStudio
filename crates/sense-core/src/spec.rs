//! The "StepOut Ready" delivery spec: progressive MP4 (moov first, not
//! fragmented), H.264 8-bit 4:2:0 SDR at a constant frame rate, AAC audio,
//! within the preset's resolution box, with no rotation metadata.

use serde::{Deserialize, Serialize};

use crate::probe::MediaInfo;

/// Max keyframe distance before seeking and frame-accurate trims get sluggish.
pub const MAX_GOP_SECONDS: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Preset {
    // Same spelling as the CLI's --preset and the app's settings.
    #[serde(rename = "720")]
    P720,
    #[default]
    #[serde(rename = "1080")]
    P1080,
    #[serde(rename = "2160")]
    P2160,
}

impl Preset {
    /// (long edge, short edge) of the bounding box; portrait video is boxed the same way.
    pub fn bounds(self) -> (u32, u32) {
        match self {
            Preset::P720 => (1280, 720),
            Preset::P1080 => (1920, 1080),
            Preset::P2160 => (3840, 2160),
        }
    }

    /// VBV peak in kbit/s so files stream smoothly from a CDN.
    pub fn max_bitrate_kbps(self) -> u32 {
        match self {
            Preset::P720 => 5_000,
            Preset::P1080 => 10_000,
            Preset::P2160 => 40_000,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Preset::P720 => "720p",
            Preset::P1080 => "1080p",
            Preset::P2160 => "2160p",
        }
    }
}

/// What it takes to resolve a violation, cheapest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fix {
    /// Informational; does not block upload.
    Advisory,
    Remux,
    AudioEncode,
    VideoEncode,
    /// Cannot be produced from this source.
    Unfixable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    NoVideo,
    Container,
    DashBrand,
    Fragmented,
    MoovAtEnd,
    VideoCodec,
    VideoProfile,
    PixelFormat,
    Hdr,
    Resolution,
    VariableFrameRate,
    Rotation,
    LongGop,
    AudioCodec,
    NoAudio,
    ExtraStreams,
}

#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub rule: Rule,
    pub fix: Fix,
    pub message: String,
}

impl Violation {
    fn new(rule: Rule, fix: Fix, message: impl Into<String>) -> Self {
        Self { rule, fix, message: message.into() }
    }

    pub fn is_blocking(&self) -> bool {
        self.fix != Fix::Advisory
    }
}

const PLAYABLE_PROFILES: [&str; 4] = ["Constrained Baseline", "Baseline", "Main", "High"];
const PLAYABLE_PIX_FMTS: [&str; 2] = ["yuv420p", "yuvj420p"];

pub fn check(info: &MediaInfo, preset: Preset) -> Vec<Violation> {
    use Fix::*;
    let mut out = Vec::new();

    let Some(video) = &info.video else {
        out.push(Violation::new(Rule::NoVideo, Unfixable, "No video stream"));
        return out;
    };

    match &info.layout {
        None => out.push(Violation::new(Rule::Container, Remux, format!("Container is {}, not MP4", info.format_name))),
        Some(layout) => {
            let brand = layout.major_brand.as_deref().unwrap_or("");
            if brand.eq_ignore_ascii_case("qt") {
                out.push(Violation::new(Rule::Container, Remux, "QuickTime MOV container, not MP4"));
            }
            if brand.eq_ignore_ascii_case("dash") {
                out.push(Violation::new(Rule::DashBrand, Remux, "major_brand is dash; progressive web players reject it"));
            }
            if layout.fragmented {
                out.push(Violation::new(Rule::Fragmented, Remux, "Fragmented MP4 (moof boxes); seeking breaks in progressive playback"));
            }
            if layout.moov_before_mdat == Some(false) {
                out.push(Violation::new(Rule::MoovAtEnd, Remux, "moov atom at end; playback cannot start until fully downloaded"));
            }
        }
    }

    if video.codec != "h264" {
        out.push(Violation::new(Rule::VideoCodec, VideoEncode, format!("Video codec {} is not H.264", video.codec)));
    } else if let Some(profile) = video.profile.as_deref().filter(|p| !PLAYABLE_PROFILES.contains(p)) {
        out.push(Violation::new(Rule::VideoProfile, VideoEncode, format!("H.264 profile {profile} is not widely decodable")));
    }
    if let Some(pix) = video.pix_fmt.as_deref().filter(|p| !PLAYABLE_PIX_FMTS.contains(p)) {
        out.push(Violation::new(Rule::PixelFormat, VideoEncode, format!("Pixel format {pix} (needs 8-bit 4:2:0)")));
    }
    if video.is_hdr() {
        let transfer = video.color_transfer.as_deref().unwrap_or("");
        out.push(Violation::new(Rule::Hdr, VideoEncode, format!("HDR ({transfer}) looks washed out on SDR screens")));
    }

    let (w, h) = video.display_size();
    let (long, short) = preset.bounds();
    if w.max(h) > long || w.min(h) > short {
        out.push(Violation::new(Rule::Resolution, VideoEncode, format!("{w}x{h} exceeds {} preset", preset.label())));
    }
    if video.rotation != 0 {
        out.push(Violation::new(Rule::Rotation, VideoEncode, format!("Rotation {}° stored as metadata only", video.rotation)));
    }
    if let Some(timing) = &video.timing {
        if timing.variable_frame_rate {
            out.push(Violation::new(Rule::VariableFrameRate, VideoEncode, "Variable frame rate; seeking and overlays drift"));
        }
        if let Some(gop) = timing.max_keyframe_interval.filter(|g| *g > MAX_GOP_SECONDS + 0.05) {
            out.push(Violation::new(Rule::LongGop, Advisory, format!("Keyframes up to {gop:.1}s apart; seeking is slower")));
        }
    }

    match &info.audio {
        None => out.push(Violation::new(Rule::NoAudio, Advisory, "No audio track")),
        Some(audio) if audio.codec != "aac" => {
            out.push(Violation::new(Rule::AudioCodec, AudioEncode, format!("Audio codec {} is not AAC", audio.codec)));
        }
        Some(_) => {}
    }
    if info.extra_streams > 0 {
        out.push(Violation::new(
            Rule::ExtraStreams,
            Advisory,
            format!("{} extra stream(s) will be dropped when fixing", info.extra_streams),
        ));
    }
    out
}

//! Turns spec violations into the cheapest FFmpeg job: stream copy when only
//! the container is wrong, audio-only encode when only audio is wrong, and a
//! full video encode only when the picture itself must change.

use std::ffi::{OsStr, OsString};
use std::path::Path;

use serde::Serialize;

use crate::caps::{Capabilities, Encoder, HwEncoder};
use crate::probe::{MediaInfo, VideoStream};
use crate::spec::{self, Fix, Preset, Violation, MAX_GOP_SECONDS};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    NoOp,
    Remux,
    AudioTranscode,
    Transcode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Fps {
    pub num: u32,
    pub den: u32,
}

impl Fps {
    pub fn as_f64(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VideoEncode {
    pub width: u32,
    pub height: u32,
    /// Output frame rate; always set so GOP length can be expressed in frames.
    pub fps: Fps,
    /// Force constant frame rate (source was VFR or above 60 fps).
    pub resample_fps: bool,
    pub tonemap: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum VideoOp {
    Copy,
    Encode(VideoEncode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioOp {
    None,
    Copy,
    Encode,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub kind: PlanKind,
    pub preset: Preset,
    pub video: VideoOp,
    pub audio: AudioOp,
    pub violations: Vec<Violation>,
}

pub fn build(info: &MediaInfo, preset: Preset) -> Result<Plan> {
    let violations = spec::check(info, preset);
    let worst = violations.iter().map(|v| v.fix).max().unwrap_or(Fix::Advisory);
    if worst == Fix::Unfixable {
        return Err(Error::NoVideo);
    }
    let video = info.video.as_ref().ok_or(Error::NoVideo)?;
    let needs = |fix: Fix| violations.iter().any(|v| v.fix == fix);

    let video_op = if needs(Fix::VideoEncode) { VideoOp::Encode(encode_settings(video, preset)) } else { VideoOp::Copy };
    let audio_op = match &info.audio {
        None => AudioOp::None,
        Some(_) if needs(Fix::AudioEncode) => AudioOp::Encode,
        Some(_) => AudioOp::Copy,
    };
    let kind = match worst {
        Fix::Advisory => PlanKind::NoOp,
        Fix::Remux => PlanKind::Remux,
        Fix::AudioEncode => PlanKind::AudioTranscode,
        Fix::VideoEncode | Fix::Unfixable => PlanKind::Transcode,
    };
    Ok(Plan { kind, preset, video: video_op, audio: audio_op, violations })
}

fn encode_settings(video: &VideoStream, preset: Preset) -> VideoEncode {
    let (width, height) = fit_within(video.display_size(), preset.bounds());
    let source_fps = video.avg_fps.unwrap_or(30.0);
    let vfr = video.timing.as_ref().is_some_and(|t| t.variable_frame_rate);
    let resample_fps = vfr || source_fps > 60.5;
    VideoEncode { width, height, fps: snap_fps(source_fps), resample_fps, tonemap: video.is_hdr() }
}

/// Scale to fit the (long, short) box without upscaling; dimensions are even for 4:2:0.
pub fn fit_within((w, h): (u32, u32), (long, short): (u32, u32)) -> (u32, u32) {
    let (w, h) = (f64::from(w.max(2)), f64::from(h.max(2)));
    let factor = (f64::from(long) / w.max(h)).min(f64::from(short) / w.min(h)).min(1.0);
    let even = |x: f64| (((x * factor) / 2.0).round() as u32 * 2).max(2);
    (even(w), even(h))
}

const STANDARD_RATES: [Fps; 8] = [
    Fps { num: 24000, den: 1001 },
    Fps { num: 24, den: 1 },
    Fps { num: 25, den: 1 },
    Fps { num: 30000, den: 1001 },
    Fps { num: 30, den: 1 },
    Fps { num: 50, den: 1 },
    Fps { num: 60000, den: 1001 },
    Fps { num: 60, den: 1 },
];

/// Nearest broadcast rate; high-speed sources (120/240) are halved down to <= 60.
pub fn snap_fps(mut fps: f64) -> Fps {
    if !fps.is_finite() || fps <= 0.0 {
        fps = 30.0;
    }
    while fps > 60.5 {
        fps /= 2.0;
    }
    STANDARD_RATES
        .into_iter()
        .min_by(|a, b| (a.as_f64() - fps).abs().total_cmp(&(b.as_f64() - fps).abs()))
        .unwrap_or(Fps { num: 30, den: 1 })
}

impl Plan {
    /// Full FFmpeg argument list. Progress is written as key=value lines to stdout.
    pub fn ffmpeg_args(&self, input: &Path, output: &Path, encoder: Encoder, caps: &Capabilities) -> Vec<OsString> {
        let mut args: Vec<OsString> = Vec::new();
        let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));

        push(&["-hide_banner", "-nostdin", "-y", "-v", "error", "-progress", "pipe:1", "-nostats", "-i"]);
        args.push(input.as_os_str().to_owned());
        let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));
        push(&["-map", "0:v:0"]);
        if self.audio != AudioOp::None {
            push(&["-map", "0:a:0"]);
        }

        match &self.video {
            VideoOp::Copy => push(&["-c:v", "copy"]),
            VideoOp::Encode(enc) => {
                let filters = video_filters(enc, encoder, caps);
                push(&["-vf", &filters]);
                push_encoder_args(&mut push, enc.fps, encoder, self.preset);
            }
        }

        match self.audio {
            AudioOp::None => {}
            AudioOp::Copy => push(&["-c:a", "copy"]),
            AudioOp::Encode => push(&["-c:a", "aac", "-b:a", "160k", "-ar", "48000", "-ac", "2"]),
        }

        push(&["-movflags", "+faststart", "-f", "mp4"]);
        args.push(output.as_os_str().to_owned());
        args
    }
}

fn video_filters(enc: &VideoEncode, encoder: Encoder, caps: &Capabilities) -> String {
    let mut chain: Vec<String> = Vec::new();
    if enc.tonemap {
        chain.push(tonemap_chain(caps).into());
    }
    if enc.resample_fps {
        chain.push(format!("fps={}/{}", enc.fps.num, enc.fps.den));
    }
    // Autorotation has already been applied by the decoder, so dimensions here are display dimensions.
    chain.push(format!("scale={}:{}:flags=bicubic", enc.width, enc.height));
    chain.push("setsar=1".into());
    chain.push(format!("format={}", output_pix_fmt(encoder)));
    chain.join(",")
}

/// HDR (PQ/HLG) to SDR BT.709, ending with explicit stream tags.
pub(crate) fn tonemap_chain(caps: &Capabilities) -> &'static str {
    if caps.zscale {
        "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv,\
         setparams=color_primaries=bt709:color_trc=bt709:colorspace=bt709:range=tv"
    } else {
        // Without zimg only the matrix can be converted; HLG survives this reasonably, PQ looks flat.
        // FFmpeg 8 tags the stream from frame properties and ignores -color_trc etc., so retag here.
        "scale=out_color_matrix=bt709:out_range=tv,setparams=color_primaries=bt709:color_trc=bt709:colorspace=bt709:range=tv"
    }
}

pub(crate) fn output_pix_fmt(encoder: Encoder) -> &'static str {
    match encoder {
        Encoder::Hw(hw) => hw.pix_fmt(),
        Encoder::X264 => "yuv420p",
    }
}

pub(crate) fn push_encoder_args(push: &mut impl FnMut(&[&str]), fps: Fps, encoder: Encoder, preset: Preset) {
    let gop = ((fps.as_f64() * MAX_GOP_SECONDS).round() as u32).max(1).to_string();
    let maxrate = preset.max_bitrate_kbps();
    let maxrate_s = format!("{maxrate}k");
    let bufsize_s = format!("{}k", maxrate * 2);
    // Constant-quality modes are not available on every hardware encoder; target 80% of the cap instead.
    let target_s = format!("{}k", maxrate * 4 / 5);

    match encoder {
        Encoder::X264 => push(&[
            "-c:v", "libx264", "-preset", "faster", "-crf", "20", "-profile:v", "high",
            "-maxrate", &maxrate_s, "-bufsize", &bufsize_s, "-g", &gop, "-keyint_min", &gop, "-sc_threshold", "0",
        ]),
        Encoder::Hw(HwEncoder::VideoToolbox) => push(&[
            "-c:v", "h264_videotoolbox", "-b:v", &target_s, "-maxrate", &maxrate_s, "-bufsize", &bufsize_s,
            "-profile:v", "high", "-g", &gop,
        ]),
        Encoder::Hw(HwEncoder::Nvenc) => push(&[
            "-c:v", "h264_nvenc", "-preset", "p5", "-rc", "vbr", "-cq", "21", "-b:v", "0",
            "-maxrate", &maxrate_s, "-bufsize", &bufsize_s, "-profile:v", "high", "-g", &gop,
        ]),
        Encoder::Hw(HwEncoder::Qsv) => push(&[
            "-c:v", "h264_qsv", "-global_quality", "22", "-maxrate", &maxrate_s, "-bufsize", &bufsize_s,
            "-profile:v", "high", "-g", &gop,
        ]),
        Encoder::Hw(HwEncoder::Amf) => push(&[
            "-c:v", "h264_amf", "-quality", "balanced", "-rc", "vbr_peak", "-b:v", &target_s,
            "-maxrate", &maxrate_s, "-bufsize", &bufsize_s, "-profile:v", "high", "-g", &gop,
        ]),
    }
}

/// Human-readable command for logs and the CSV report.
pub fn display_args(args: &[OsString]) -> String {
    args.iter().map(|a| a.as_os_str()).map(OsStr::to_string_lossy).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_landscape_and_portrait_without_upscaling() {
        assert_eq!(fit_within((3840, 2160), (1920, 1080)), (1920, 1080));
        assert_eq!(fit_within((2160, 3840), (1920, 1080)), (1080, 1920));
        assert_eq!(fit_within((1280, 720), (1920, 1080)), (1280, 720));
        assert_eq!(fit_within((4096, 2160), (1920, 1080)), (1920, 1012));
        assert_eq!(fit_within((641, 361), (1920, 1080)), (642, 362));
    }

    #[test]
    fn snaps_frame_rates() {
        assert_eq!(snap_fps(29.83), Fps { num: 30000, den: 1001 });
        assert_eq!(snap_fps(119.88), Fps { num: 60000, den: 1001 });
        assert_eq!(snap_fps(240.0), Fps { num: 60, den: 1 });
        assert_eq!(snap_fps(25.0), Fps { num: 25, den: 1 });
        assert_eq!(snap_fps(f64::NAN), Fps { num: 30, den: 1 });
    }
}

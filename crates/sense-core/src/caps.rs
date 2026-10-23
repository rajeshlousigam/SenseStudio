//! What the FFmpeg build on this machine can actually do. Hardware encoders
//! are confirmed with a tiny test encode: being compiled in does not mean a
//! usable GPU or driver is present.

use std::ffi::OsStr;

use serde::{Deserialize, Serialize};

use crate::tools::{command, run_capture, Tools};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HwEncoder {
    VideoToolbox,
    Nvenc,
    Qsv,
    Amf,
}

impl HwEncoder {
    const ALL: [HwEncoder; 4] = [HwEncoder::VideoToolbox, HwEncoder::Nvenc, HwEncoder::Qsv, HwEncoder::Amf];

    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            HwEncoder::VideoToolbox => "h264_videotoolbox",
            HwEncoder::Nvenc => "h264_nvenc",
            HwEncoder::Qsv => "h264_qsv",
            HwEncoder::Amf => "h264_amf",
        }
    }

    pub(crate) fn pix_fmt(self) -> &'static str {
        match self {
            HwEncoder::Qsv => "nv12",
            _ => "yuv420p",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoder {
    X264,
    Hw(HwEncoder),
}

impl Encoder {
    pub fn name(self) -> &'static str {
        match self {
            Encoder::X264 => "libx264",
            Encoder::Hw(hw) => hw.ffmpeg_name(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EncoderPref {
    /// Hardware when available (fast on low-end CPUs), else libx264.
    #[default]
    Auto,
    Software,
    Hardware,
}

#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    pub x264: bool,
    /// Usable hardware encoders, in preference order.
    pub hardware: Vec<HwEncoder>,
    /// zimg-backed scaler, required for accurate HDR to SDR tonemapping.
    pub zscale: bool,
}

impl Capabilities {
    pub fn detect(tools: &Tools) -> Result<Self> {
        let encoders = run_capture("ffmpeg", &tools.ffmpeg, &["-hide_banner".as_ref(), "-encoders".as_ref()])?;
        let filters = run_capture("ffmpeg", &tools.ffmpeg, &["-hide_banner".as_ref(), "-filters".as_ref()])?;
        let has_encoder = |name: &str| encoders.lines().any(|l| l.split_whitespace().nth(1) == Some(name));

        let hardware = HwEncoder::ALL
            .into_iter()
            .filter(|hw| has_encoder(hw.ffmpeg_name()) && test_encode(tools, *hw))
            .collect();

        Ok(Self {
            x264: has_encoder("libx264"),
            hardware,
            zscale: filters.lines().any(|l| l.split_whitespace().nth(1) == Some("zscale")),
        })
    }

    pub fn pick(&self, pref: EncoderPref) -> Result<Encoder> {
        let hw = self.hardware.first().copied().map(Encoder::Hw);
        let sw = self.x264.then_some(Encoder::X264);
        match pref {
            EncoderPref::Auto => hw.or(sw),
            EncoderPref::Software => sw,
            EncoderPref::Hardware => hw,
        }
        .ok_or(Error::NoEncoder)
    }
}

fn test_encode(tools: &Tools, hw: HwEncoder) -> bool {
    let args: [&OsStr; 16] = [
        "-hide_banner".as_ref(),
        "-v".as_ref(),
        "error".as_ref(),
        "-f".as_ref(),
        "lavfi".as_ref(),
        "-i".as_ref(),
        "color=c=black:s=256x256:r=30:d=0.2".as_ref(),
        "-pix_fmt".as_ref(),
        hw.pix_fmt().as_ref(),
        "-c:v".as_ref(),
        hw.ffmpeg_name().as_ref(),
        "-frames:v".as_ref(),
        "5".as_ref(),
        "-f".as_ref(),
        "null".as_ref(),
        "-".as_ref(),
    ];
    command(&tools.ffmpeg)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .is_ok_and(|o| o.status.success())
}

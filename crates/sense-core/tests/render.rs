//! Trim/merge rendering and proxies against generated clips.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use sense_core::caps::{Capabilities, Encoder};
use sense_core::probe::probe;
use sense_core::render::{self, RenderRequest, RenderSpec, Segment};
use sense_core::spec::{self, Preset};
use sense_core::tools::Tools;
use tempfile::TempDir;

fn tools() -> Tools {
    Tools::from_path().expect("ffmpeg and ffprobe must be on PATH to run media tests")
}

fn ffmpeg(args: &[&str]) {
    let status = Command::new(&tools().ffmpeg).args(["-v", "error", "-y"]).args(args).status().unwrap();
    assert!(status.success(), "fixture command failed: {args:?}");
}

fn clip(dir: &Path, name: &str, size: &str, seconds: u32, audio: bool) -> PathBuf {
    let out = dir.join(name);
    let video = format!("testsrc2=s={size}:r=30:d={seconds}");
    let tone = format!("sine=f=440:r=48000:d={seconds}");
    let mut args = vec!["-f", "lavfi", "-i", &video];
    if audio {
        args.extend(["-f", "lavfi", "-i", &tone, "-c:a", "aac", "-shortest"]);
    }
    args.extend(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-g", "30", "-movflags", "+faststart"]);
    let out_s = out.to_str().unwrap().to_owned();
    args.push(&out_s);
    ffmpeg(&args);
    out
}

fn segment(path: &Path, start: f64, end: Option<f64>) -> Segment {
    Segment { path: path.to_owned(), start, end }
}

fn run(spec: &RenderSpec, dir: &Path) -> sense_core::Result<render::RenderOutcome> {
    let tools = tools();
    let caps = Capabilities::detect(&tools).unwrap();
    let output = dir.join("out.mp4");
    let cancel = AtomicBool::new(false);
    let req = RenderRequest { tools: &tools, caps: &caps, encoder: Encoder::X264, spec, output: &output, cancel: &cancel };
    render::render(&req, &mut |_| {})
}

#[test]
fn trims_and_merges_mixed_sources_into_one_ready_clip() {
    let dir = TempDir::new().unwrap();
    let a = clip(dir.path(), "a.mp4", "640x360", 6, true);
    let b = clip(dir.path(), "b.mp4", "320x240", 4, false); // different size, no audio track
    let spec = RenderSpec { segments: vec![segment(&a, 1.0, Some(4.0)), segment(&b, 0.0, None)], preset: Preset::P1080 };

    let outcome = run(&spec, dir.path()).unwrap();
    assert!((outcome.expected_duration - 7.0).abs() < 1e-9);
    let after = probe(&tools(), &outcome.output).unwrap();
    assert!((after.duration.unwrap() - 7.0).abs() < 0.3, "duration {:?}", after.duration);
    let video = after.video.as_ref().unwrap();
    assert_eq!((video.width, video.height), (640, 360), "output takes the first clip's size");
    assert!(after.audio.is_some(), "a silent clip must still produce an audio track");
    assert!(spec::check(&after, Preset::P1080).iter().all(|v| !v.is_blocking()));
}

#[test]
fn rejects_empty_ranges_without_writing_output() {
    let dir = TempDir::new().unwrap();
    let a = clip(dir.path(), "a.mp4", "320x240", 3, true);
    let spec = RenderSpec { segments: vec![segment(&a, 2.0, Some(1.0))], preset: Preset::P1080 };
    assert!(run(&spec, dir.path()).unwrap_err().to_string().contains("empty"));
    assert!(!dir.path().join("out.mp4").exists());
    let none = RenderSpec { segments: vec![], preset: Preset::P1080 };
    assert!(run(&none, dir.path()).is_err());
}

#[test]
fn proxy_is_540p_all_intra_h264() {
    let dir = TempDir::new().unwrap();
    let src = clip(dir.path(), "src.mp4", "1280x720", 2, true);
    let out = dir.path().join("proxy.mp4");
    render::make_proxy(&tools(), &src, &out, &AtomicBool::new(false), &mut |_| {}).unwrap();
    let video = probe(&tools(), &out).unwrap().video.unwrap();
    assert_eq!((video.codec.as_str(), video.width, video.height), ("h264", 960, 540));
    assert!(video.timing.unwrap().max_keyframe_interval.unwrap() < 0.05);
}

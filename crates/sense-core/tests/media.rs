//! End-to-end checks against small clips generated with the local FFmpeg.
//! Each fixture reproduces one real-world failure seen on the platform.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use sense_core::caps::{Capabilities, Encoder};
use sense_core::pipeline::{fix_file, FixOutcome, FixRequest};
use sense_core::plan::{self, PlanKind, VideoOp};
use sense_core::probe::probe;
use sense_core::spec::{self, Preset, Rule};
use sense_core::tools::Tools;
use tempfile::TempDir;

fn tools() -> Tools {
    Tools::from_path().expect("ffmpeg and ffprobe must be on PATH to run media tests")
}

fn ffmpeg(args: &[&str]) {
    let status = Command::new(&tools().ffmpeg).args(["-v", "error", "-y"]).args(args).status().unwrap();
    assert!(status.success(), "fixture command failed: {args:?}");
}

fn path_str(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// 3 s 320x240 H.264 + AAC with faststart: the shape every fix must end up in.
fn ready(dir: &Path) -> PathBuf {
    let out = dir.join("ready.mp4");
    ffmpeg(&[
        "-f", "lavfi", "-i", "testsrc2=s=320x240:r=30:d=3", "-f", "lavfi", "-i", "sine=f=440:r=48000:d=3",
        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "30", "-c:a", "aac", "-shortest", "-movflags", "+faststart",
        path_str(&out),
    ]);
    out
}

fn derive(dir: &Path, name: &str, pre_input: &[&str], src: &Path, args: &[&str]) -> PathBuf {
    let out = dir.join(name);
    let mut full: Vec<&str> = pre_input.to_vec();
    full.extend(["-i", path_str(src)]);
    full.extend(args);
    full.push(path_str(&out));
    ffmpeg(&full);
    out
}

fn rules(path: &Path, preset: Preset) -> Vec<Rule> {
    let info = probe(&tools(), path).unwrap();
    spec::check(&info, preset).into_iter().filter(|v| v.is_blocking()).map(|v| v.rule).collect()
}

/// Fix with libx264 so results don't depend on the machine's GPU.
fn fix(path: &Path, dir: &Path, preset: Preset) -> FixOutcome {
    let tools = tools();
    let caps = Capabilities::detect(&tools).unwrap();
    let output = dir.join(format!("{}_fixed.mp4", path.file_stem().unwrap().to_string_lossy()));
    let before = probe(&tools, path).unwrap();
    let cancel = AtomicBool::new(false);
    let req = FixRequest { tools: &tools, caps: &caps, encoder: Encoder::X264, preset, input: path, output: &output, cancel: &cancel };
    fix_file(&req, &before, &mut |_| {}).unwrap()
}

fn assert_fixed(outcome: &FixOutcome, expected: PlanKind) -> PathBuf {
    match outcome {
        FixOutcome::Fixed { plan, output, .. } => {
            assert_eq!(plan.kind, expected);
            assert!(rules(output, plan.preset).is_empty(), "fixed output still has violations");
            assert!(!output.with_extension("mp4.partial").exists());
            output.clone()
        }
        FixOutcome::AlreadyReady { .. } => panic!("expected a fix, source was already ready"),
    }
}

#[test]
fn ready_file_is_left_alone() {
    let dir = TempDir::new().unwrap();
    let src = ready(dir.path());
    assert!(rules(&src, Preset::P1080).is_empty());
    assert!(matches!(fix(&src, dir.path(), Preset::P1080), FixOutcome::AlreadyReady { .. }));
}

#[test]
fn dash_fragmented_file_is_remuxed() {
    let dir = TempDir::new().unwrap();
    let src = ready(dir.path());
    let dash = derive(dir.path(), "dash.mp4", &[], &src, &[
        "-c", "copy", "-movflags", "+frag_keyframe+empty_moov+default_base_moof", "-brand", "dash",
    ]);
    let found = rules(&dash, Preset::P1080);
    assert!(found.contains(&Rule::DashBrand) && found.contains(&Rule::Fragmented), "{found:?}");
    assert_fixed(&fix(&dash, dir.path(), Preset::P1080), PlanKind::Remux);
}

#[test]
fn moov_at_end_is_remuxed() {
    let dir = TempDir::new().unwrap();
    let src = ready(dir.path());
    let tail = derive(dir.path(), "tail.mp4", &[], &src, &["-c", "copy"]);
    assert_eq!(rules(&tail, Preset::P1080), vec![Rule::MoovAtEnd]);
    assert_fixed(&fix(&tail, dir.path(), Preset::P1080), PlanKind::Remux);
}

#[test]
fn mkv_container_is_remuxed() {
    let dir = TempDir::new().unwrap();
    let src = ready(dir.path());
    let mkv = derive(dir.path(), "clip.mkv", &[], &src, &["-c", "copy"]);
    assert_eq!(rules(&mkv, Preset::P1080), vec![Rule::Container]);
    assert_fixed(&fix(&mkv, dir.path(), Preset::P1080), PlanKind::Remux);
}

#[test]
fn non_aac_audio_only_reencodes_audio() {
    let dir = TempDir::new().unwrap();
    let src = ready(dir.path());
    let mp3 = derive(dir.path(), "mp3.mp4", &[], &src, &["-c:v", "copy", "-c:a", "libmp3lame", "-movflags", "+faststart"]);
    let info = probe(&tools(), &mp3).unwrap();
    let plan = plan::build(&info, Preset::P1080).unwrap();
    assert_eq!(plan.video, VideoOp::Copy);
    assert_fixed(&fix(&mp3, dir.path(), Preset::P1080), PlanKind::AudioTranscode);
}

#[test]
fn rotation_metadata_is_baked_in() {
    let dir = TempDir::new().unwrap();
    let src = ready(dir.path());
    let rotated = derive(dir.path(), "rot.mp4", &["-display_rotation", "90"], &src, &["-c", "copy", "-movflags", "+faststart"]);
    assert_eq!(rules(&rotated, Preset::P1080), vec![Rule::Rotation]);
    let out = assert_fixed(&fix(&rotated, dir.path(), Preset::P1080), PlanKind::Transcode);
    let video = probe(&tools(), &out).unwrap().video.unwrap();
    assert_eq!((video.width, video.height, video.rotation), (240, 320, 0));
}

#[test]
fn variable_frame_rate_becomes_constant() {
    let dir = TempDir::new().unwrap();
    let vfr = dir.path().join("vfr.mp4");
    ffmpeg(&[
        "-f", "lavfi", "-i", "testsrc2=s=320x240:r=30:d=4", "-vf", "select='lt(mod(n\\,10)\\,6)'", "-fps_mode", "vfr",
        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-movflags", "+faststart", path_str(&vfr),
    ]);
    assert!(rules(&vfr, Preset::P1080).contains(&Rule::VariableFrameRate));
    assert_fixed(&fix(&vfr, dir.path(), Preset::P1080), PlanKind::Transcode);
}

#[test]
fn hdr_hevc_10bit_becomes_sdr_h264() {
    let dir = TempDir::new().unwrap();
    let hdr = dir.path().join("hlg.mp4");
    ffmpeg(&[
        // FFmpeg 8 takes colour tags from frame properties, so set them on the frames, not as output flags.
        "-f", "lavfi", "-i", "testsrc2=s=320x240:r=30:d=2",
        "-vf", "setparams=color_primaries=bt2020:color_trc=arib-std-b67:colorspace=bt2020nc",
        "-c:v", "libx265", "-x265-params", "log-level=none", "-pix_fmt", "yuv420p10le",
        "-tag:v", "hvc1", "-movflags", "+faststart", path_str(&hdr),
    ]);
    let found = rules(&hdr, Preset::P1080);
    for rule in [Rule::VideoCodec, Rule::PixelFormat, Rule::Hdr] {
        assert!(found.contains(&rule), "missing {rule:?} in {found:?}");
    }
    let out = assert_fixed(&fix(&hdr, dir.path(), Preset::P1080), PlanKind::Transcode);
    let video = probe(&tools(), &out).unwrap().video.unwrap();
    assert_eq!(video.codec, "h264");
    assert_eq!(video.color_transfer.as_deref(), Some("bt709"));
}

#[test]
fn uhd_is_downscaled_to_preset() {
    let dir = TempDir::new().unwrap();
    let uhd = dir.path().join("uhd.mp4");
    ffmpeg(&[
        "-f", "lavfi", "-i", "testsrc2=s=3840x2160:r=30:d=1", "-c:v", "libx264", "-preset", "ultrafast",
        "-pix_fmt", "yuv420p", "-movflags", "+faststart", path_str(&uhd),
    ]);
    assert_eq!(rules(&uhd, Preset::P1080), vec![Rule::Resolution]);
    assert!(rules(&uhd, Preset::P2160).is_empty());
    let out = assert_fixed(&fix(&uhd, dir.path(), Preset::P720), PlanKind::Transcode);
    let video = probe(&tools(), &out).unwrap().video.unwrap();
    assert_eq!((video.width, video.height), (1280, 720));
}

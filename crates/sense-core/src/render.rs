//! Trim and merge: segments from one or more sources, in order, normalised to
//! one size and frame rate and encoded to the StepOut spec. A single segment
//! covering a whole file simply re-encodes it to the spec.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use crate::caps::{Capabilities, Encoder};
use crate::exec::{run_ffmpeg, Progress};
use crate::plan::{fit_within, output_pix_fmt, push_encoder_args, snap_fps, tonemap_chain, Fps};
use crate::probe::{probe, MediaInfo};
use crate::spec::{self, Preset};
use crate::tools::Tools;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub path: PathBuf,
    /// Source seconds; defaults to the start of the file.
    #[serde(default)]
    pub start: f64,
    /// Source seconds; defaults to the end of the file.
    #[serde(default)]
    pub end: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderSpec {
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub preset: Preset,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderOutcome {
    pub output: PathBuf,
    pub expected_duration: f64,
    pub after: Box<MediaInfo>,
}

struct Resolved {
    path: PathBuf,
    info: MediaInfo,
    start: f64,
    end: f64,
}

impl Resolved {
    fn len(&self) -> f64 {
        self.end - self.start
    }
}

pub struct RenderRequest<'a> {
    pub tools: &'a Tools,
    pub caps: &'a Capabilities,
    pub encoder: Encoder,
    pub spec: &'a RenderSpec,
    pub output: &'a Path,
    pub cancel: &'a AtomicBool,
}

pub fn render(req: &RenderRequest<'_>, on_progress: &mut dyn FnMut(Progress)) -> Result<RenderOutcome> {
    let spec = req.spec;
    if spec.segments.is_empty() {
        return Err(invalid("no clips to export"));
    }
    let resolved = spec
        .segments
        .iter()
        .enumerate()
        .map(|(i, seg)| resolve(req.tools, i, seg))
        .collect::<Result<Vec<_>>>()?;

    let expected: f64 = resolved.iter().map(Resolved::len).sum();
    let partial = partial_path(req.output);
    let graph_file = req.output.with_extension("filtergraph.txt");
    let (inputs, graph) = build_graph(spec.preset, &resolved, req.encoder, req.caps);
    fs::write(&graph_file, graph)?;
    let args = ffmpeg_args(&inputs, &graph_file, &partial, req.encoder, spec.preset, target_fps(&resolved));

    let result = run_ffmpeg(&req.tools.ffmpeg, &args, Some(expected), req.cancel, on_progress)
        .and_then(|()| probe(req.tools, &partial))
        .and_then(|after| verify(&after, expected, spec.preset).map(|()| after));
    let _ = fs::remove_file(&graph_file);

    match result {
        Ok(mut after) => {
            fs::rename(&partial, req.output)?;
            after.path = req.output.to_owned();
            Ok(RenderOutcome { output: req.output.to_owned(), expected_duration: expected, after: Box::new(after) })
        }
        Err(err) => {
            // Best effort: the original error matters more than a failed cleanup.
            let _ = fs::remove_file(&partial);
            Err(err)
        }
    }
}

fn invalid(message: &str) -> Error {
    Error::Parse { what: "export", message: message.to_owned() }
}

fn resolve(tools: &Tools, index: usize, seg: &Segment) -> Result<Resolved> {
    let label = |m: &str| invalid(&format!("clip {}: {m}", index + 1));
    let info = probe(tools, &seg.path)?;
    if info.video.is_none() {
        return Err(label("no video stream"));
    }
    let duration = info.duration.ok_or_else(|| label("duration unknown"))?;
    let start = seg.start.max(0.0);
    let end = seg.end.unwrap_or(duration).min(duration);
    if end - start < 0.04 {
        return Err(label("empty (out point must be after in point)"));
    }
    Ok(Resolved { path: seg.path.clone(), info, start, end })
}

fn target_fps(resolved: &[Resolved]) -> Fps {
    snap_fps(resolved[0].info.video.as_ref().and_then(|v| v.avg_fps).unwrap_or(30.0))
}

/// Returns (input args, filter graph). One seeked input per segment: `-ss` before `-i`
/// jumps to the nearest keyframe and FFmpeg decodes forward to the exact frame, so a clip
/// late in a 90-minute match does not decode the whole match.
fn build_graph(preset: Preset, resolved: &[Resolved], encoder: Encoder, caps: &Capabilities) -> (Vec<OsString>, String) {
    let display = resolved[0].info.video.as_ref().map_or((1920, 1080), |v| v.display_size());
    let (w, h) = fit_within(display, preset.bounds());
    let fps = target_fps(resolved);
    let mut inputs: Vec<OsString> = Vec::new();
    let mut graph: Vec<String> = Vec::new();
    let mut concat = String::new();

    for (i, r) in resolved.iter().enumerate() {
        inputs.extend(["-ss".into(), format!("{:.6}", r.start).into(), "-t".into(), format!("{:.6}", r.len()).into()]);
        inputs.extend(["-i".into(), r.path.clone().into_os_string()]);
        let hdr = r.info.video.as_ref().is_some_and(|v| v.is_hdr());
        let tonemap = if hdr { format!("{},", tonemap_chain(caps)) } else { String::new() };
        // Clips of another size are letterboxed onto the first clip's frame.
        graph.push(format!(
            "[{i}:v:0]{tonemap}setpts=PTS-STARTPTS,fps={}/{},scale={w}:{h}:force_original_aspect_ratio=decrease:flags=bicubic,\
             pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1,format=yuv420p[v{i}]",
            fps.num, fps.den
        ));
        let len = r.len();
        // Every clip needs audio of exactly its own length or the merge drifts; silent clips get silence.
        let audio = if r.info.audio.is_some() {
            format!("[{i}:a:0]asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo")
        } else {
            "anullsrc=r=48000:cl=stereo".to_owned()
        };
        graph.push(format!("{audio},apad=whole_dur={len:.6},atrim=duration={len:.6}[a{i}]"));
        concat.push_str(&format!("[v{i}][a{i}]"));
    }
    graph.push(format!("{concat}concat=n={}:v=1:a=1[vcat][aout]", resolved.len()));
    graph.push(format!("[vcat]format={}[vout]", output_pix_fmt(encoder)));
    (inputs, graph.join(";\n"))
}

fn ffmpeg_args(inputs: &[OsString], graph_file: &Path, output: &Path, encoder: Encoder, preset: Preset, fps: Fps) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["-hide_banner", "-nostdin", "-y", "-v", "error", "-progress", "pipe:1", "-nostats"]
        .iter()
        .map(OsString::from)
        .collect();
    args.extend(inputs.iter().cloned());
    // `-/opt file` reads the value from a file, keeping long merges off the command line (Windows limits it).
    args.push("-/filter_complex".into());
    args.push(graph_file.as_os_str().to_owned());
    let mut rest: Vec<String> = vec!["-map".into(), "[vout]".into(), "-map".into(), "[aout]".into()];
    push_encoder_args(&mut |items: &[&str]| rest.extend(items.iter().map(|s| (*s).to_owned())), fps, encoder, preset);
    rest.extend(
        ["-c:a", "aac", "-b:a", "160k", "-ar", "48000", "-ac", "2", "-movflags", "+faststart", "-f", "mp4"]
            .iter()
            .map(|s| (*s).to_owned()),
    );
    args.extend(rest.into_iter().map(OsString::from));
    args.push(output.as_os_str().to_owned());
    args
}

fn verify(after: &MediaInfo, expected: f64, preset: Preset) -> Result<()> {
    let remaining: Vec<String> =
        spec::check(after, preset).into_iter().filter(|v| v.is_blocking()).map(|v| v.message).collect();
    if !remaining.is_empty() {
        return Err(Error::Verify(format!("output still violates spec: {}", remaining.join("; "))));
    }
    let actual = after.duration.unwrap_or(0.0);
    if (actual - expected).abs() > (expected * 0.01).max(0.5) {
        return Err(Error::Verify(format!("duration mismatch: expected {expected:.2}s, got {actual:.2}s")));
    }
    Ok(())
}

pub(crate) fn partial_path(output: &Path) -> PathBuf {
    let mut name = output.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".partial");
    output.with_file_name(name)
}

/// 540p all-intra H.264 preview copy for files the platform player cannot open
/// (e.g. MKV on macOS). Every frame is a keyframe, so seeking stays exact and cheap.
pub fn make_proxy(tools: &Tools, input: &Path, output: &Path, cancel: &AtomicBool, on_progress: &mut dyn FnMut(Progress)) -> Result<()> {
    let duration = probe(tools, input)?.duration;
    let partial = partial_path(output);
    let mut args: Vec<OsString> = ["-hide_banner", "-nostdin", "-y", "-v", "error", "-progress", "pipe:1", "-nostats", "-i"]
        .iter()
        .map(OsString::from)
        .collect();
    args.push(input.as_os_str().to_owned());
    args.extend(
        [
            "-map", "0:v:0", "-map", "0:a:0?", "-vf", "scale=-2:540:flags=fast_bilinear,format=yuv420p",
            "-c:v", "libx264", "-preset", "ultrafast", "-tune", "fastdecode", "-crf", "28", "-g", "1", "-bf", "0",
            "-c:a", "aac", "-b:a", "96k", "-ac", "2", "-movflags", "+faststart", "-f", "mp4",
        ]
        .iter()
        .map(OsString::from),
    );
    args.push(partial.as_os_str().to_owned());
    match run_ffmpeg(&tools.ffmpeg, &args, duration, cancel, on_progress) {
        Ok(()) => Ok(fs::rename(&partial, output)?),
        Err(e) => {
            let _ = fs::remove_file(&partial);
            Err(e)
        }
    }
}

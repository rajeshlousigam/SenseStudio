//! probe -> plan -> run -> verify. Output goes to a `.partial` file that is
//! only renamed into place after the result itself passes the spec.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::Serialize;

use crate::caps::{Capabilities, Encoder};
use crate::exec::{run_ffmpeg, Progress};
use crate::plan::{self, Plan, PlanKind};
use crate::probe::{probe, MediaInfo};
use crate::spec::{self, Preset};
use crate::tools::Tools;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum FixOutcome {
    /// Source already meets the spec; nothing was written.
    AlreadyReady { plan: Plan },
    Fixed { plan: Plan, output: PathBuf, after: Box<MediaInfo>, encoder: Option<Encoder> },
}

pub struct FixRequest<'a> {
    pub tools: &'a Tools,
    pub caps: &'a Capabilities,
    pub encoder: Encoder,
    pub preset: Preset,
    pub input: &'a Path,
    pub output: &'a Path,
    pub cancel: &'a AtomicBool,
}

pub fn fix_file(req: &FixRequest<'_>, before: &MediaInfo, on_progress: &mut dyn FnMut(Progress)) -> Result<FixOutcome> {
    let plan = plan::build(before, req.preset)?;
    if plan.kind == PlanKind::NoOp {
        return Ok(FixOutcome::AlreadyReady { plan });
    }

    let partial = partial_path(req.output);
    let args = plan.ffmpeg_args(req.input, &partial, req.encoder, req.caps);
    let result = run_ffmpeg(&req.tools.ffmpeg, &args, before.duration, req.cancel, on_progress)
        .and_then(|()| probe(req.tools, &partial))
        .and_then(|after| verify(before, &after, req.preset).map(|()| after));

    match result {
        Ok(mut after) => {
            fs::rename(&partial, req.output)?;
            after.path = req.output.to_owned();
            let encoder = matches!(plan.kind, PlanKind::Transcode).then_some(req.encoder);
            Ok(FixOutcome::Fixed { plan, output: req.output.to_owned(), after: Box::new(after), encoder })
        }
        Err(err) => {
            // Best effort: the original error matters more than a failed cleanup.
            let _ = fs::remove_file(&partial);
            Err(err)
        }
    }
}

/// The output must satisfy the spec on its own and keep the source duration.
pub fn verify(before: &MediaInfo, after: &MediaInfo, preset: Preset) -> Result<()> {
    if after.video.is_none() {
        return Err(Error::Verify("output has no video stream".into()));
    }
    if before.audio.is_some() && after.audio.is_none() {
        return Err(Error::Verify("audio track was lost".into()));
    }
    let remaining: Vec<String> =
        spec::check(after, preset).into_iter().filter(|v| v.is_blocking()).map(|v| v.message).collect();
    if !remaining.is_empty() {
        return Err(Error::Verify(format!("output still violates spec: {}", remaining.join("; "))));
    }
    if let (Some(b), Some(a)) = (before.duration, after.duration) {
        if b > 0.0 && (b - a).abs() > (b * 0.005).max(2.0) {
            return Err(Error::Verify(format!("duration mismatch: input {b:.2}s, output {a:.2}s")));
        }
    }
    Ok(())
}

fn partial_path(output: &Path) -> PathBuf {
    let mut name = output.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".partial");
    output.with_file_name(name)
}

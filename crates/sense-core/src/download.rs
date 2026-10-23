//! URL downloads through yt-dlp, preferring H.264/AAC in MP4 so most results
//! are already StepOut Ready or need only a remux. Only use this for content
//! you own or are licensed to download.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::Serialize;

use crate::exec::{run_lines, LineSource};
use crate::{Error, Result};

const PROGRESS_TAG: &str = "SENSEPROG ";
const FILE_TAG: &str = "SENSEFILE ";

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DownloadEvent {
    Progress { fraction: Option<f64>, speed: String, eta: String },
    File { path: PathBuf },
}

pub struct DownloadRequest<'a> {
    pub yt_dlp: &'a Path,
    /// Directory containing ffmpeg, used by yt-dlp to merge video+audio.
    pub ffmpeg_dir: Option<&'a Path>,
    pub url: &'a str,
    pub out_dir: &'a Path,
    pub max_height: u32,
    pub cancel: &'a AtomicBool,
}

pub fn download(req: &DownloadRequest<'_>, on_event: &mut dyn FnMut(DownloadEvent)) -> Result<Vec<PathBuf>> {
    if !(req.url.starts_with("http://") || req.url.starts_with("https://")) {
        return Err(Error::Parse { what: "URL", message: format!("not an http(s) URL: {}", req.url) });
    }
    let h = req.max_height;
    let mut args: Vec<OsString> = Vec::new();
    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));
    push(&["--newline", "--no-colors", "--progress", "--no-mtime"]);
    push(&["--no-playlist"]);
    let format = format!("bv*[height<={h}]+ba/b[height<={h}]/bv*+ba/b");
    push(&["-f", &format, "-S", "vcodec:h264,acodec:m4a", "--merge-output-format", "mp4"]);
    push(&["-o", "%(title).80B [%(id)s].%(ext)s"]);
    push(&["--progress-template", "download:SENSEPROG %(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s"]);
    push(&["--print", "after_move:SENSEFILE %(filepath)s"]);
    args.push("-P".into());
    args.push(req.out_dir.as_os_str().to_owned());
    if let Some(dir) = req.ffmpeg_dir {
        args.push("--ffmpeg-location".into());
        args.push(dir.as_os_str().to_owned());
    }
    args.push("--".into());
    args.push(req.url.into());

    let mut files = Vec::new();
    run_lines("yt-dlp", req.yt_dlp, &args, LineSource::Stdout, req.cancel, &mut |line| {
        if let Some(rest) = line.strip_prefix(PROGRESS_TAG) {
            on_event(parse_progress(rest));
        } else if let Some(path) = line.strip_prefix(FILE_TAG) {
            let path = PathBuf::from(path.trim());
            files.push(path.clone());
            on_event(DownloadEvent::File { path });
        }
    })?;
    if files.is_empty() {
        return Err(Error::Tool { tool: "yt-dlp", message: "finished without producing a file".into() });
    }
    Ok(files)
}

fn parse_progress(rest: &str) -> DownloadEvent {
    let mut parts = rest.split('|').map(str::trim);
    let fraction = parts
        .next()
        .and_then(|p| p.trim_end_matches('%').trim().parse::<f64>().ok())
        .map(|p| (p / 100.0).clamp(0.0, 1.0));
    DownloadEvent::Progress {
        fraction,
        speed: parts.next().unwrap_or("").to_owned(),
        eta: parts.next().unwrap_or("").to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_template() {
        match parse_progress("  42.5%|  3.10MiB/s|00:12") {
            DownloadEvent::Progress { fraction, speed, eta } => {
                assert!((fraction.unwrap() - 0.425).abs() < 1e-9);
                assert_eq!(speed, "3.10MiB/s");
                assert_eq!(eta, "00:12");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(parse_progress("N/A|N/A|N/A"), DownloadEvent::Progress { fraction: None, .. }));
    }
}

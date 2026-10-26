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
    /// Browser whose signed-in session yt-dlp borrows (chrome, safari, edge, firefox, brave).
    /// YouTube increasingly refuses anonymous downloads with "confirm you're not a bot".
    pub cookies_from_browser: Option<&'a str>,
    pub cancel: &'a AtomicBool,
}

const BROWSERS: [&str; 5] = ["chrome", "safari", "edge", "firefox", "brave"];

pub fn download(req: &DownloadRequest<'_>, on_event: &mut dyn FnMut(DownloadEvent)) -> Result<Vec<PathBuf>> {
    if !(req.url.starts_with("http://") || req.url.starts_with("https://")) {
        return Err(Error::Parse { what: "URL", message: format!("not an http(s) URL: {}", req.url) });
    }
    let h = req.max_height;
    let mut args: Vec<OsString> = Vec::new();
    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));
    push(&["--newline", "--no-colors", "--progress", "--no-mtime"]);
    push(&["--no-playlist"]);
    // A stream that is live right now would download until it ends; skip it with a clear message.
    push(&["--match-filter", "!is_live"]);
    let format = format!("bv*[height<={h}]+ba/b[height<={h}]/bv*+ba/b");
    push(&["-f", &format, "-S", "vcodec:h264,acodec:m4a", "--merge-output-format", "mp4"]);
    push(&["-o", "%(title).80B [%(id)s].%(ext)s"]);
    push(&["--progress-template", "download:SENSEPROG %(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s"]);
    push(&["--print", "after_move:SENSEFILE %(filepath)s"]);
    args.push("-P".into());
    args.push(req.out_dir.as_os_str().to_owned());
    if let Some(browser) = req.cookies_from_browser {
        if !BROWSERS.contains(&browser) {
            return Err(Error::Parse { what: "browser", message: format!("unsupported browser: {browser}") });
        }
        args.push("--cookies-from-browser".into());
        args.push(browser.into());
    }
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
    })
    .map_err(|e| match e {
        Error::Tool { tool, message } => Error::Tool { tool, message: friendly_error(&message, req.cookies_from_browser) },
        other => other,
    })?;
    if files.is_empty() {
        return Err(Error::Tool { tool: "yt-dlp", message: "finished without producing a file".into() });
    }
    Ok(files)
}

/// yt-dlp prints warnings, errors and long FAQ links; keep the ERROR lines and explain the
/// failures people actually hit.
fn friendly_error(message: &str, browser: Option<&str>) -> String {
    let errors: Vec<&str> = message
        .lines()
        .filter_map(|l| l.trim().strip_prefix("ERROR:"))
        .map(str::trim)
        .collect();
    let text = if errors.is_empty() { message.trim().to_owned() } else { errors.join(" ") };
    if text.contains("confirm you") && text.contains("not a bot") {
        return match browser {
            None => "YouTube wants a signed-in session (\"confirm you're not a bot\"). Choose the browser you're \
                     logged into YouTube with under \"Use sign-in from\", then Retry."
                .into(),
            Some(b) => format!(
                "YouTube still refused with your {b} session. Open youtube.com in {b}, make sure you are signed in, \
                 then Retry. If it keeps failing, try another browser."
            ),
        };
    }
    if text.contains("live event has ended") || text.contains("No video formats found") || text.contains("Requested format is not available") {
        return "YouTube has no downloadable video for this link yet. If it was a live stream, the replay may still be \
                processing or the channel didn't save it; try again later. If it plays on youtube.com but still fails \
                here, choose your browser under \"Use sign-in from\" and Retry."
            .into();
    }
    if text.contains("live event will begin") || text.contains("Premieres in") {
        return "This stream hasn't started yet. Download it after it has finished.".into();
    }
    if text.contains("does not pass filter") && text.contains("is_live") {
        return "This stream is live right now. Download it after it ends and the replay is available.".into();
    }
    if text.contains("Operation not permitted") && browser == Some("safari") {
        return "macOS blocked access to Safari's cookies. Allow SenseStudio in System Settings → Privacy & Security → \
                Full Disk Access, or choose Chrome or Firefox."
            .into();
    }
    if text.contains("could not find") && text.contains("cookies database") {
        return format!("No {} profile with cookies was found on this computer. Choose another browser.", browser.unwrap_or("browser"));
    }
    if text.contains("Failed to decrypt") || text.contains("Could not copy") && text.contains("cookie database") {
        return format!(
            "Couldn't read {}'s cookies (the browser may be open and locking them, or blocks access). \
             Close it and Retry, or choose Firefox.",
            browser.unwrap_or("the browser")
        );
    }
    text
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
    fn explains_bot_checks_and_keeps_only_error_lines() {
        let raw = "WARNING: [youtube] No title found\nERROR: [youtube] abc: Sign in to confirm you’re not a bot. Use --cookies-from-browser";
        assert!(friendly_error(raw, None).contains("Use sign-in from"));
        assert!(friendly_error(raw, Some("chrome")).contains("chrome session"));
        assert_eq!(friendly_error("WARNING: x\nERROR: [generic] HTTP Error 404", None), "[generic] HTTP Error 404");
        assert!(friendly_error("ERROR: [youtube] abc: This live event has ended.", None).contains("replay may still be processing"));
    }

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

//! Single-job commands used by the desktop app: render, proxy, download.
//! With --events they speak the same NDJSON start/progress/result protocol as `fix`.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::Ordering;

use sense_core::caps::{Capabilities, EncoderPref};
use sense_core::download::{self, DownloadEvent, DownloadRequest};
use sense_core::exec::Progress;
use sense_core::render::{self, RenderRequest, RenderSpec};
use sense_core::spec::Preset;
use sense_core::tools::{find_on_path, Tools};
use sense_core::Error;

use crate::{emit, watch_stdin_for_cancel, CANCEL};

fn status_for(err: &Error) -> &'static str {
    if matches!(err, Error::Cancelled) {
        "CANCELLED"
    } else {
        "ERROR"
    }
}

fn progress_printer(events: bool, index: usize) -> impl FnMut(Progress) {
    let mut last = -1i64;
    move |p: Progress| {
        let Some(fraction) = p.fraction else { return };
        let pct = (fraction * 100.0) as i64;
        if pct == last {
            return;
        }
        last = pct;
        if events {
            emit(serde_json::json!({ "event": "progress", "index": index, "fraction": fraction, "speed": p.speed }));
        } else {
            eprint!("\r  {pct:3}%{}   ", p.speed.map(|s| format!(" {s:.1}x")).unwrap_or_default());
            let _ = io::stderr().flush();
        }
    }
}

fn finish(events: bool, result: Result<serde_json::Value, Error>) -> ExitCode {
    match result {
        Ok(mut ok) => {
            if events {
                ok["event"] = "result".into();
                ok["index"] = 0.into();
                emit(ok);
            } else {
                eprintln!();
                println!("{ok}");
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            if events {
                emit(serde_json::json!({ "event": "result", "index": 0, "status": status_for(&err), "details": err.to_string() }));
            } else {
                eprintln!("\nerror: {err}");
            }
            ExitCode::from(1)
        }
    }
}

pub fn render_cmd(tools: &Tools, spec_path: &Path, output: &Path, pref: EncoderPref, events: bool) -> Result<ExitCode, String> {
    let text = fs::read_to_string(spec_path).map_err(|e| format!("cannot read {}: {e}", spec_path.display()))?;
    let spec: RenderSpec = serde_json::from_str(&text).map_err(|e| format!("invalid render spec: {e}"))?;
    let caps = Capabilities::detect(tools).map_err(|e| e.to_string())?;
    let encoder = caps.pick(pref).map_err(|e| e.to_string())?;
    if let Some(dir) = output.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    if events {
        watch_stdin_for_cancel();
        emit(serde_json::json!({ "event": "start", "index": 0, "total": 1, "source": spec_path }));
    }
    let req = RenderRequest { tools, caps: &caps, encoder, spec: &spec, output, cancel: &CANCEL };
    let result = render::render(&req, &mut progress_printer(events, 0)).map(|outcome| {
        serde_json::json!({
            "status": "RENDERED",
            "output_file": outcome.output,
            "duration": outcome.after.duration,
            "expected_duration": outcome.expected_duration,
            "details": format!("{} via {}: {}", spec.preset.label(), encoder.name(), outcome.after.streams_summary()),
        })
    });
    Ok(finish(events, result))
}

pub fn proxy_cmd(tools: &Tools, input: &Path, output: &Path, events: bool) -> Result<ExitCode, String> {
    if let Some(dir) = output.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    if events {
        watch_stdin_for_cancel();
        emit(serde_json::json!({ "event": "start", "index": 0, "total": 1, "source": input }));
    }
    let result = render::make_proxy(tools, input, output, &CANCEL, &mut progress_printer(events, 0))
        .map(|()| serde_json::json!({ "status": "PROXY_READY", "output_file": output, "details": "540p preview proxy" }));
    Ok(finish(events, result))
}

pub fn download_cmd(tools: &Tools, urls: &[String], out_dir: &Path, preset: Preset, events: bool) -> Result<ExitCode, String> {
    let yt_dlp = find_on_path("yt-dlp").map_err(|_| "yt-dlp was not found next to the app or on PATH".to_owned())?;
    fs::create_dir_all(out_dir).map_err(|e| format!("cannot create {}: {e}", out_dir.display()))?;
    if events {
        watch_stdin_for_cancel();
    }
    let ffmpeg_dir = tools.ffmpeg.parent().map(Path::to_path_buf);
    let max_height = preset.bounds().1;
    let mut failures = 0;

    for (index, url) in urls.iter().enumerate() {
        if CANCEL.load(Ordering::Relaxed) {
            break;
        }
        if events {
            emit(serde_json::json!({ "event": "start", "index": index, "total": urls.len(), "source": url }));
        } else {
            eprintln!("[{}/{}] {url}", index + 1, urls.len());
        }
        let req = DownloadRequest {
            yt_dlp: &yt_dlp,
            ffmpeg_dir: ffmpeg_dir.as_deref(),
            url,
            out_dir,
            max_height,
            cancel: &CANCEL,
        };
        let mut last = -1i64;
        let result = download::download(&req, &mut |event| match event {
            DownloadEvent::Progress { fraction, speed, eta } => {
                let pct = fraction.map_or(-1, |f| (f * 100.0) as i64);
                if pct == last {
                    return;
                }
                last = pct;
                if events {
                    emit(serde_json::json!({ "event": "progress", "index": index, "fraction": fraction, "speed_text": speed, "eta": eta }));
                } else {
                    eprint!("\r  {pct:3}%  {speed}  ETA {eta}   ");
                    let _ = io::stderr().flush();
                }
            }
            DownloadEvent::File { path } => {
                if events {
                    emit(serde_json::json!({ "event": "file", "index": index, "path": path }));
                }
            }
        });
        let value = match result {
            Ok(files) => serde_json::json!({ "status": "DOWNLOADED", "files": files, "details": format!("{} file(s)", files.len()) }),
            Err(err) => {
                failures += 1;
                serde_json::json!({ "status": status_for(&err), "files": Vec::<PathBuf>::new(), "details": err.to_string() })
            }
        };
        if events {
            let mut value = value;
            value["event"] = "result".into();
            value["index"] = index.into();
            emit(value);
        } else {
            eprintln!();
            println!("{value}");
        }
    }
    Ok(if failures > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

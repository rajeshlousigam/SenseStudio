//! Runs external tools (FFmpeg, yt-dlp) with line-by-line output and a cancel
//! flag. Readers live on their own threads so a chatty stderr can never fill
//! its pipe and deadlock the child.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use serde::Serialize;

use crate::tools::{command, tail};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Progress {
    /// Seconds of output written so far.
    pub out_time: f64,
    /// 0..=1 when the expected output duration is known.
    pub fraction: Option<f64>,
    /// Encoding speed relative to real time.
    pub speed: Option<f64>,
}

/// Which stream `run_lines` hands to the callback; the other is kept as an error tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineSource {
    Stdout,
    Stderr,
}

pub fn run_lines(
    tool: &'static str,
    program: &Path,
    args: &[OsString],
    source: LineSource,
    cancel: &AtomicBool,
    on_line: &mut dyn FnMut(&str),
) -> Result<()> {
    let mut child = command(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return Err(Error::Tool { tool, message: "could not capture output pipes".into() });
    };
    let (lines, rest): (Box<dyn Read + Send>, Box<dyn Read + Send>) = match source {
        LineSource::Stdout => (Box::new(stdout), Box::new(stderr)),
        LineSource::Stderr => (Box::new(stderr), Box::new(stdout)),
    };

    let rest_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut rest = rest;
        // Lossy: tools can print non-UTF-8 file names.
        let _ = rest.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    });

    let (tx, rx) = mpsc::channel::<String>();
    let line_reader = thread::spawn(move || {
        let mut reader = BufReader::new(lines);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    // yt-dlp and ffmpeg rewrite progress lines with \r; treat it as a line break too.
                    for part in String::from_utf8_lossy(&buf).split(['\r', '\n']).filter(|p| !p.is_empty()) {
                        if tx.send(part.to_owned()).is_err() {
                            return;
                        }
                    }
                }
            }
        }
    });

    let mut seen = Vec::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            // Already-exited is fine; we only care that it is gone.
            let _ = child.kill();
            let _ = child.wait();
            let _ = line_reader.join();
            let _ = rest_reader.join();
            return Err(Error::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                on_line(&line);
                // Keep a short tail of the callback stream for error messages too.
                seen.push(line);
                if seen.len() > 40 {
                    seen.remove(0);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    let status = child.wait()?;
    let _ = line_reader.join();
    let rest_text = rest_reader.join().unwrap_or_default();
    if !status.success() {
        let text = if rest_text.trim().is_empty() { seen.join("\n") } else { rest_text };
        return Err(Error::Tool { tool, message: tail(&text, 1500) });
    }
    Ok(())
}

/// FFmpeg with `-progress pipe:1`; `duration` is the expected output length.
pub fn run_ffmpeg(
    ffmpeg: &Path,
    args: &[OsString],
    duration: Option<f64>,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<()> {
    let mut progress = Progress::default();
    run_lines("ffmpeg", ffmpeg, args, LineSource::Stdout, cancel, &mut |line| {
        let Some((key, value)) = line.split_once('=') else { return };
        match key {
            "out_time_us" => {
                if let Ok(us) = value.trim().parse::<i64>() {
                    progress.out_time = us.max(0) as f64 / 1_000_000.0;
                    progress.fraction = duration.filter(|d| *d > 0.0).map(|d| (progress.out_time / d).clamp(0.0, 1.0));
                }
            }
            "speed" => progress.speed = value.trim().trim_end_matches('x').parse().ok(),
            "progress" => {
                // out_time stops short of the input duration on the final block; report completion.
                if value.trim() == "end" && progress.fraction.is_some() {
                    progress.fraction = Some(1.0);
                }
                on_progress(progress);
            }
            _ => {}
        }
    })
}

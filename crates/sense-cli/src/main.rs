//! Command-line front end for sense-core; supersedes utils/remux_check.py.
//! Never uploads anything: fixed files and a CSV report land in --output-dir.

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

mod jobs;
mod remote;

use clap::{Args, Parser, Subcommand, ValueEnum};
use sense_core::caps::{Capabilities, Encoder, EncoderPref};
use sense_core::pipeline::{fix_file, FixOutcome, FixRequest};
use sense_core::plan::{self, PlanKind};
use sense_core::probe::{probe, probe_remote, MediaInfo};
use sense_core::spec::Preset;
use sense_core::tools::Tools;

#[derive(Parser)]
#[command(name = "sense-cli", version, about = "Check and fix videos against the StepOut Ready spec")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Report what each file needs without changing anything.
    Check(Common),
    /// Remux or re-encode files that fail the spec, verifying every output.
    Fix {
        #[command(flatten)]
        common: Common,
        #[arg(long, default_value = "./sense-output")]
        output_dir: PathBuf,
        #[arg(long, value_enum, default_value_t = EncoderArg::Auto)]
        encoder: EncoderArg,
    },
    /// Print one JSON object per file or link: media info plus the fix plan.
    /// Links are checked with range requests; nothing is downloaded.
    Probe {
        inputs: Vec<String>,
        #[arg(long, value_enum, default_value_t = PresetArg::P1080)]
        preset: PresetArg,
    },
    /// Trim and merge clips (JSON spec) into one StepOut Ready file.
    Render {
        spec: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, value_enum, default_value_t = EncoderArg::Auto)]
        encoder: EncoderArg,
        #[arg(long)]
        events: bool,
    },
    /// Make a 540p all-intra preview copy for files the platform player cannot open.
    Proxy {
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        events: bool,
    },
    /// Download videos with yt-dlp. Only for content you own or are licensed to download.
    Download {
        urls: Vec<String>,
        #[arg(long)]
        output_dir: PathBuf,
        /// Caps the downloaded resolution.
        #[arg(long, value_enum, default_value_t = PresetArg::P1080)]
        preset: PresetArg,
        #[arg(long)]
        events: bool,
    },
    /// Show detected encoders and filters.
    Caps,
}

#[derive(Args)]
struct Common {
    /// Local files or http(s) URLs.
    inputs: Vec<String>,
    /// Text file with one path or URL per line (# comments and Markdown links allowed).
    #[arg(long)]
    list: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = PresetArg::P1080)]
    preset: PresetArg,
    /// CSV report path (fix defaults to <output-dir>/sense-report.csv).
    #[arg(long)]
    report: Option<PathBuf>,
    /// Bucket name used only to print a suggested s3:// replacement for URL inputs.
    #[arg(long)]
    bucket: Option<String>,
    /// Network connect/read timeout in seconds for URL inputs.
    #[arg(long, default_value_t = 180)]
    timeout: u64,
    /// Print one JSON object per input to stdout.
    #[arg(long)]
    json: bool,
    /// Machine mode for the desktop app: NDJSON start/progress/result events on stdout.
    /// Closing stdin (or writing "cancel") stops the current job and kills FFmpeg.
    #[arg(long)]
    events: bool,
}

/// Set by the stdin watcher in --events mode; checked by the FFmpeg runner.
pub(crate) static CANCEL: AtomicBool = AtomicBool::new(false);

pub(crate) fn emit(value: serde_json::Value) {
    let mut out = io::stdout().lock();
    // A closed stdout means the app went away; the stdin watcher handles cancellation.
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

pub(crate) fn watch_stdin_for_cancel() {
    std::thread::spawn(|| {
        let stdin = io::stdin();
        let mut line = String::new();
        loop {
            line.clear();
            match stdin.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) if line.trim() == "cancel" => break,
                Ok(_) => {}
            }
        }
        CANCEL.store(true, std::sync::atomic::Ordering::Relaxed);
    });
}

#[derive(Clone, Copy, ValueEnum)]
enum PresetArg {
    #[value(name = "720")]
    P720,
    #[value(name = "1080")]
    P1080,
    #[value(name = "2160")]
    P2160,
}

impl From<PresetArg> for Preset {
    fn from(p: PresetArg) -> Self {
        match p {
            PresetArg::P720 => Preset::P720,
            PresetArg::P1080 => Preset::P1080,
            PresetArg::P2160 => Preset::P2160,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum EncoderArg {
    Auto,
    Software,
    Hardware,
}

impl From<EncoderArg> for EncoderPref {
    fn from(e: EncoderArg) -> Self {
        match e {
            EncoderArg::Auto => EncoderPref::Auto,
            EncoderArg::Software => EncoderPref::Software,
            EncoderArg::Hardware => EncoderPref::Hardware,
        }
    }
}

#[derive(Default)]
struct Row {
    source: String,
    source_filename: String,
    major_brand: String,
    format_name: String,
    duration_seconds: String,
    size_bytes: String,
    streams: String,
    status: String,
    plan: String,
    violations: String,
    output_file: String,
    suggested_s3_replacement: String,
    details: String,
}

const CSV_HEADER: [&str; 13] = [
    "source", "source_filename", "major_brand", "format_name", "duration_seconds", "size_bytes", "streams", "status",
    "plan", "violations", "output_file", "suggested_s3_replacement", "details",
];

impl Row {
    fn fields(&self) -> [&str; 13] {
        [
            &self.source, &self.source_filename, &self.major_brand, &self.format_name, &self.duration_seconds,
            &self.size_bytes, &self.streams, &self.status, &self.plan, &self.violations, &self.output_file,
            &self.suggested_s3_replacement, &self.details,
        ]
    }

    fn fill_from(&mut self, info: &MediaInfo) {
        self.major_brand = info.major_brand().unwrap_or("").to_owned();
        self.format_name = info.format_name.clone();
        self.duration_seconds = info.duration.map(|d| format!("{d:.3}")).unwrap_or_default();
        self.size_bytes = info.size.map(|s| s.to_string()).unwrap_or_default();
        self.streams = info.streams_summary();
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, String> {
    let tools = Tools::from_path().map_err(|e| e.to_string())?;
    match cli.cmd {
        Cmd::Caps => {
            let caps = Capabilities::detect(&tools).map_err(|e| e.to_string())?;
            println!("ffmpeg:   {}", tools.ffmpeg.display());
            println!("libx264:  {}", caps.x264);
            println!("hardware: {}", caps.hardware.iter().map(|h| h.ffmpeg_name()).collect::<Vec<_>>().join(", "));
            println!("zscale:   {} (accurate HDR tonemapping)", caps.zscale);
            let auto = caps.pick(EncoderPref::Auto).map(Encoder::name).unwrap_or("none");
            println!("auto picks: {auto}");
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Probe { inputs, preset } => {
            let preset: Preset = preset.into();
            let agent = http_agent(60);
            for source in inputs {
                let probed = if is_url(&source) {
                    remote_probe(&tools, &agent, &source)
                } else {
                    probe(&tools, Path::new(&source)).map_err(|e| e.to_string())
                };
                let line = match probed {
                    Ok(info) => match plan::build(&info, preset) {
                        Ok(plan) => serde_json::json!({ "source": source, "info": info, "plan": plan }),
                        Err(e) => serde_json::json!({ "source": source, "info": info, "error": e.to_string() }),
                    },
                    Err(e) => serde_json::json!({ "source": source, "error": e }),
                };
                emit(line);
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Render { spec, output, encoder, events } => jobs::render_cmd(&tools, &spec, &output, encoder.into(), events),
        Cmd::Proxy { input, output, events } => jobs::proxy_cmd(&tools, &input, &output, events),
        Cmd::Download { urls, output_dir, preset, events } => jobs::download_cmd(&tools, &urls, &output_dir, preset.into(), events),
        Cmd::Check(common) => process(&tools, &common, None),
        Cmd::Fix { common, output_dir, encoder } => {
            let caps = Capabilities::detect(&tools).map_err(|e| e.to_string())?;
            let encoder = caps.pick(encoder.into()).map_err(|e| e.to_string())?;
            fs::create_dir_all(&output_dir).map_err(|e| format!("cannot create {}: {e}", output_dir.display()))?;
            let output_dir = output_dir.canonicalize().map_err(|e| e.to_string())?;
            eprintln!("Encoder for re-encodes: {}{}", encoder.name(), if caps.zscale { "" } else { " (no zscale: HDR tonemap is approximate)" });
            process(&tools, &common, Some(FixContext { caps, encoder, output_dir }))
        }
    }
}

struct FixContext {
    caps: Capabilities,
    encoder: Encoder,
    output_dir: PathBuf,
}

fn process(tools: &Tools, common: &Common, fix: Option<FixContext>) -> Result<ExitCode, String> {
    let mut inputs = common.inputs.clone();
    if let Some(list) = &common.list {
        inputs.extend(read_list(list)?);
    }
    if inputs.is_empty() {
        return Err("no inputs given (pass files/URLs or --list)".into());
    }
    let preset: Preset = common.preset.into();
    let report_path = common.report.clone().or_else(|| fix.as_ref().map(|f| f.output_dir.join("sense-report.csv")));
    let agent = http_agent(common.timeout);
    let cancel = &CANCEL;
    if common.events {
        watch_stdin_for_cancel();
    }

    let mut rows = Vec::new();
    let (mut errors, mut not_ready) = (0usize, 0usize);
    for (i, input) in inputs.iter().enumerate() {
        eprintln!("[{}/{}] {input}", i + 1, inputs.len());
        if common.events {
            emit(serde_json::json!({ "event": "start", "index": i, "total": inputs.len(), "source": input }));
        }
        let mut row = Row { source: input.clone(), ..Row::default() };
        let result = process_one(tools, &agent, common, preset, fix.as_ref(), cancel, i, input, &mut row);
        match result {
            Ok(ready) => {
                if !ready {
                    not_ready += 1;
                }
                eprintln!("  {} {}", row.status, row.details);
            }
            Err(message) => {
                let cancelled = cancel.load(std::sync::atomic::Ordering::Relaxed);
                if !cancelled {
                    errors += 1;
                }
                row.status = if cancelled { "CANCELLED" } else { "ERROR" }.into();
                row.details = message;
                eprintln!("  {} {}", row.status, row.details);
            }
        }
        if common.events {
            let mut event = row_object(&row);
            event.insert("event".into(), "result".into());
            event.insert("index".into(), i.into());
            emit(serde_json::Value::Object(event));
        } else if common.json {
            println!("{}", serde_json::Value::Object(row_object(&row)));
        }
        rows.push(row);
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!("Cancelled; skipping remaining inputs.");
            break;
        }
    }

    if let Some(path) = report_path {
        write_csv(&path, &rows).map_err(|e| format!("cannot write report {}: {e}", path.display()))?;
        eprintln!("Report: {}", path.display());
    }
    eprintln!("{} input(s): {} error(s), {} not ready", rows.len(), errors, not_ready);
    Ok(if errors > 0 {
        ExitCode::from(1)
    } else if fix.is_none() && not_ready > 0 {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    })
}

/// Returns whether the source (check) or the result (fix) is StepOut Ready.
#[allow(clippy::too_many_arguments)]
fn process_one(
    tools: &Tools,
    agent: &ureq::Agent,
    common: &Common,
    preset: Preset,
    fix: Option<&FixContext>,
    cancel: &AtomicBool,
    index: usize,
    input: &str,
    row: &mut Row,
) -> Result<bool, String> {
    // Keeps the download alive until this input is finished.
    let _download_dir;
    let local: PathBuf = if is_url(input) {
        let filename = url_filename(input).unwrap_or_else(|| "download.mp4".into());
        row.source_filename = filename.clone();
        if let (Some(bucket), Some(key)) = (&common.bucket, url_key(input)) {
            row.suggested_s3_replacement = format!("s3://{bucket}/{key}");
        }
        // Check the link in place first; only a file that needs fixing is downloaded.
        eprintln!("  checking link...");
        let remote = remote_probe(tools, agent, input)?;
        row.fill_from(&remote);
        let plan = plan::build(&remote, preset).map_err(|e| e.to_string())?;
        row.plan = kind_label(plan.kind).into();
        row.violations = plan.violations.iter().map(|v| v.message.as_str()).collect::<Vec<_>>().join(" | ");
        if fix.is_none() || plan.kind == PlanKind::NoOp {
            row.status = if plan.kind == PlanKind::NoOp { "READY" } else { needs_label(plan.kind) }.into();
            if fix.is_some() {
                row.details = "Link already meets the spec; nothing downloaded.".into();
            }
            return Ok(plan.kind == PlanKind::NoOp);
        }
        let dir = tempfile::Builder::new().prefix("sense-dl-").tempdir().map_err(|e| e.to_string())?;
        let dest = dir.path().join(&filename);
        eprintln!("  downloading...");
        download(agent, input, &dest, cancel, &mut |fraction| {
            if common.events {
                emit(serde_json::json!({ "event": "progress", "index": index, "fraction": fraction, "speed": null, "stage": "download" }));
            }
        })?;
        _download_dir = Some(dir);
        dest
    } else {
        _download_dir = None;
        let path = PathBuf::from(input);
        row.source_filename = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        path
    };

    let before = probe(tools, &local).map_err(|e| e.to_string())?;
    row.fill_from(&before);

    let Some(ctx) = fix else {
        let plan = plan::build(&before, preset).map_err(|e| e.to_string())?;
        row.plan = kind_label(plan.kind).into();
        row.violations = plan.violations.iter().map(|v| v.message.as_str()).collect::<Vec<_>>().join(" | ");
        row.status = if plan.kind == PlanKind::NoOp { "READY" } else { needs_label(plan.kind) }.into();
        return Ok(plan.kind == PlanKind::NoOp);
    };

    let stem = Path::new(&row.source_filename).file_stem().map(|s| s.to_string_lossy().into_owned());
    let output = unique_output(&ctx.output_dir, stem.as_deref().unwrap_or("video"));
    let req = FixRequest {
        tools,
        caps: &ctx.caps,
        encoder: ctx.encoder,
        preset,
        input: &local,
        output: &output,
        cancel,
    };
    let mut last_pct = -1i64;
    let outcome = fix_file(&req, &before, &mut |p| {
        let Some(fraction) = p.fraction else { return };
        let pct = (fraction * 100.0) as i64;
        if pct != last_pct {
            last_pct = pct;
            if common.events {
                emit(serde_json::json!({ "event": "progress", "index": index, "fraction": fraction, "speed": p.speed }));
                return;
            }
            let speed = p.speed.map(|s| format!(" {s:.1}x")).unwrap_or_default();
            eprint!("\r  {pct:3}%{speed}   ");
            let _ = io::stderr().flush();
        }
    });
    if last_pct >= 0 && !common.events {
        eprintln!();
    }
    let outcome = outcome.map_err(|e| e.to_string())?;

    match outcome {
        FixOutcome::AlreadyReady { plan } => {
            row.plan = kind_label(plan.kind).into();
            row.violations = plan.violations.iter().map(|v| v.message.as_str()).collect::<Vec<_>>().join(" | ");
            row.status = "READY".into();
            row.details = "Already meets the spec; no output written.".into();
        }
        FixOutcome::Fixed { plan, output, after, encoder } => {
            row.plan = kind_label(plan.kind).into();
            row.violations = plan.violations.iter().map(|v| v.message.as_str()).collect::<Vec<_>>().join(" | ");
            row.status = match plan.kind {
                PlanKind::Remux => "REMUXED_VALIDATED",
                _ => "TRANSCODED_VALIDATED",
            }
            .into();
            row.output_file = output.display().to_string();
            let via = encoder.map(|e| format!(" via {}", e.name())).unwrap_or_default();
            row.details = format!("Output {}{via}: {}", preset.label(), after.streams_summary());
        }
    }
    Ok(true)
}

fn kind_label(kind: PlanKind) -> &'static str {
    match kind {
        PlanKind::NoOp => "none",
        PlanKind::Remux => "remux",
        PlanKind::AudioTranscode => "audio_transcode",
        PlanKind::Transcode => "transcode",
    }
}

fn needs_label(kind: PlanKind) -> &'static str {
    match kind {
        PlanKind::Remux => "NEEDS_REMUX",
        PlanKind::AudioTranscode => "NEEDS_AUDIO_TRANSCODE",
        _ => "NEEDS_TRANSCODE",
    }
}

fn unique_output(dir: &Path, stem: &str) -> PathBuf {
    let first = dir.join(format!("{stem}_ready.mp4"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{stem}_ready_{n}.mp4")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

fn is_url(input: &str) -> bool {
    input.starts_with("http://") || input.starts_with("https://")
}

fn http_agent(timeout_secs: u64) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(timeout_secs))
        .timeout_read(Duration::from_secs(timeout_secs))
        .user_agent(concat!("SenseStudio/", env!("CARGO_PKG_VERSION")))
        .build()
}

fn remote_probe(tools: &Tools, agent: &ureq::Agent, url: &str) -> Result<MediaInfo, String> {
    let mut reader = remote::HttpRangeReader::open(agent, url)?;
    let mut info = probe_remote(tools, url, &mut reader).map_err(|e| e.to_string())?;
    if info.size.is_none() && reader.len() > 0 {
        info.size = Some(reader.len());
    }
    Ok(info)
}

/// Streams to disk in 1 MiB chunks, reporting progress per percent and honouring cancel.
fn download(agent: &ureq::Agent, url: &str, dest: &Path, cancel: &AtomicBool, on_progress: &mut dyn FnMut(f64)) -> Result<(), String> {
    let response = agent.get(url).call().map_err(|e| format!("download failed: {e}"))?;
    let total: Option<u64> = response.header("Content-Length").and_then(|v| v.parse().ok());
    let mut reader = response.into_reader();
    let mut file = BufWriter::new(File::create(dest).map_err(|e| e.to_string())?);
    let mut buf = vec![0u8; 1 << 20];
    let (mut done, mut last_pct) = (0u64, -1i64);
    loop {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let n = io::Read::read(&mut reader, &mut buf).map_err(|e| format!("download interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if let Some(total) = total.filter(|t| *t > 0) {
            let pct = (done * 100 / total) as i64;
            if pct != last_pct {
                last_pct = pct;
                on_progress(done as f64 / total as f64);
            }
        }
    }
    file.flush().map_err(|e| e.to_string())
}

fn read_list(path: &Path) -> Result<Vec<String>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Ok(text.lines().filter_map(parse_list_line).collect())
}

/// Same rules as remux_check.py: skips blanks and comments, unwraps `[label](url)`.
fn parse_list_line(raw: &str) -> Option<String> {
    let mut line = raw.trim().trim_start_matches('\u{feff}');
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    if line.ends_with(')') {
        if let Some((_, target)) = line.rsplit_once("](") {
            line = target.trim_end_matches(')').trim();
        }
    }
    let line = line.trim_matches(|c| matches!(c, '*' | '`' | '<' | '>' | ' '));
    (!line.is_empty()).then(|| line.to_owned())
}

fn url_path(url: &str) -> Option<&str> {
    let after_scheme = url.split_once("://")?.1;
    let path = after_scheme.split_once('/').map(|(_, p)| p).unwrap_or("");
    let path = path.split(['?', '#']).next().unwrap_or("");
    (!path.is_empty()).then_some(path)
}

fn url_key(url: &str) -> Option<String> {
    url_path(url).map(percent_decode)
}

fn url_filename(url: &str) -> Option<String> {
    let name = percent_decode(url_path(url)?.rsplit('/').next()?);
    let safe: String = name.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '_' } else { c }).collect();
    (!safe.is_empty() && safe != "." && safe != "..").then_some(safe)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn write_csv(path: &Path, rows: &[Row]) -> io::Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    writeln!(out, "{}", CSV_HEADER.join(","))?;
    for row in rows {
        let line: Vec<String> = row.fields().iter().map(|f| csv_field(f)).collect();
        writeln!(out, "{}", line.join(","))?;
    }
    out.flush()
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn row_object(row: &Row) -> serde_json::Map<String, serde_json::Value> {
    CSV_HEADER.iter().zip(row.fields()).map(|(k, v)| ((*k).to_owned(), v.into())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_list_lines_like_remux_check() {
        assert_eq!(parse_list_line("# comment"), None);
        assert_eq!(parse_list_line("  "), None);
        assert_eq!(parse_list_line("[clip](https://x.com/a.mp4)").as_deref(), Some("https://x.com/a.mp4"));
        assert_eq!(parse_list_line("<https://x.com/b.mp4>").as_deref(), Some("https://x.com/b.mp4"));
        assert_eq!(parse_list_line("/tmp/local file.mp4").as_deref(), Some("/tmp/local file.mp4"));
    }

    #[test]
    fn derives_names_and_keys_from_urls() {
        let url = "https://videos.example.com/videos/match%201.mp4?sig=abc";
        assert_eq!(url_key(url).as_deref(), Some("videos/match 1.mp4"));
        assert_eq!(url_filename(url).as_deref(), Some("match 1.mp4"));
        assert_eq!(url_filename("https://host/"), None);
        assert_eq!(percent_decode("a%2"), "a%2");
    }

    #[test]
    fn quotes_csv_fields() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a, \"b\""), "\"a, \"\"b\"\"\"");
    }
}

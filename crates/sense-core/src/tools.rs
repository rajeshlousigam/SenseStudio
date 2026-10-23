use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Error, Result};

/// Locations of the FFmpeg binaries. The desktop app passes its bundled
/// sidecars; the CLI resolves them from PATH.
#[derive(Debug, Clone)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl Tools {
    pub fn new(ffmpeg: impl Into<PathBuf>, ffprobe: impl Into<PathBuf>) -> Self {
        Self { ffmpeg: ffmpeg.into(), ffprobe: ffprobe.into() }
    }

    pub fn from_path() -> Result<Self> {
        Ok(Self { ffmpeg: find_on_path("ffmpeg")?, ffprobe: find_on_path("ffprobe")? })
    }
}

/// Resolve an executable from PATH (adds `.exe` on Windows).
pub fn find_on_path(name: &str) -> Result<PathBuf> {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() };
    let path = std::env::var_os("PATH").ok_or_else(|| Error::ToolMissing(name.to_owned()))?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(&exe))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| Error::ToolMissing(name.to_owned()))
}

/// A Command that does not flash a console window when spawned from the GUI on Windows.
pub(crate) fn command(program: &Path) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Run a short-lived tool to completion, returning stdout or the stderr tail on failure.
pub(crate) fn run_capture(tool: &'static str, program: &Path, args: &[&std::ffi::OsStr]) -> Result<String> {
    let output = command(program).args(args).output()?;
    if !output.status.success() {
        return Err(Error::Tool { tool, message: tail(&String::from_utf8_lossy(&output.stderr), 1500) });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(crate) fn tail(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let count = trimmed.chars().count();
    if count <= max_chars {
        return if trimmed.is_empty() { "unknown error".to_owned() } else { trimmed.to_owned() };
    }
    trimmed.chars().skip(count - max_chars).collect()
}

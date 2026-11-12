use anyhow::{Context, Result, bail};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Owns the OS child even when reading, waiting or validation exits early.
pub struct ManagedChild(pub std::process::Child);
impl std::ops::Deref for ManagedChild {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for ManagedChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for ManagedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn command(program: &Path) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW, not a shell.
    }
    command
}

pub fn tool(name: &str) -> Result<PathBuf> {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let root = std::env::current_exe()?
        .parent()
        .context("Missing application directory")?
        .to_owned();
    for candidate in [root.join(&file), root.join("tools").join(&file)] {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    // The portable Windows distribution owns its media binaries; do not load arbitrary PATH tools.
    #[cfg(not(windows))]
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let candidate = dir.join(&file);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    bail!(
        "{file} is missing. Keep the tools folder next to Cutline and extract the entire downloaded ZIP."
    )
}

/// Capture both pipes concurrently. Deadline and bounded buffers prevent corrupt media from
/// hanging a worker or consuming unbounded memory. Children are killed and reaped on timeout.
pub fn capture(mut command: Command, timeout: Duration, limit: usize) -> Result<Vec<u8>> {
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Could not start the media tool")?;
    let mut child = ManagedChild(child);
    let stdout = child.stdout.take().context("Missing media output pipe")?;
    let stderr = child.stderr.take().context("Missing diagnostic pipe")?;
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.take(65536).read_to_end(&mut bytes).map(|_| bytes)
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let bytes = out
        .join()
        .map_err(|_| anyhow::anyhow!("Media reader stopped"))??;
    let diagnostics = err
        .join()
        .map_err(|_| anyhow::anyhow!("Diagnostic reader stopped"))??;
    let Some(status) = status else {
        bail!("Reading this file took too long. Try a local copy or another file.");
    };
    if !status.success() {
        bail!("{}", String::from_utf8_lossy(&diagnostics).trim());
    }
    if bytes.len() > limit {
        bail!("Media information exceeded the safe size limit");
    }
    Ok(bytes)
}

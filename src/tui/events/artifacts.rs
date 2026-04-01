use anyhow::Result;
use std::fs::{self, create_dir_all};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub(super) fn copy_to_clipboard(value: &str) -> Result<()> {
    let mut child = Command::new("pbcopy").stdin(Stdio::piped()).spawn()?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(value.as_bytes())?;
    }
    let _ = child.wait()?;
    Ok(())
}

pub(super) fn save_text_artifact(prefix: &str, extension: &str, value: &str) -> Result<PathBuf> {
    let dir = std::env::current_dir()?.join("downloads").join("artifacts");
    create_dir_all(&dir)?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let path = dir.join(format!("{prefix}-{ts}.{extension}"));
    fs::write(&path, value)?;
    Ok(path)
}

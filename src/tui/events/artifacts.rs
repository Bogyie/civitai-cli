use anyhow::Result;
use std::fs::{self, create_dir_all};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub(super) fn copy_to_clipboard(value: &str) -> Result<()> {
    let commands: Vec<(&str, Vec<&str>)> = if cfg!(target_os = "macos") {
        vec![("pbcopy", vec![])]
    } else {
        vec![
            ("wl-copy", vec![]),
            ("xclip", vec!["-selection", "clipboard"]),
            ("xsel", vec!["--clipboard", "--input"]),
        ]
    };

    let mut last_error = None;
    for (program, args) in commands {
        match Command::new(program).args(args).stdin(Stdio::piped()).spawn() {
            Ok(mut child) => {
                if let Some(stdin) = child.stdin.as_mut() {
                    stdin.write_all(value.as_bytes())?;
                }
                let status = child.wait()?;
                if status.success() {
                    return Ok(());
                }
                last_error = Some(anyhow::anyhow!("{program} exited with status {status}"));
            }
            Err(err) => {
                last_error = Some(err.into());
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("No clipboard command available")))
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

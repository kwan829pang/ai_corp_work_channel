use anyhow::{Context, Result};
use chrono::Local;
use std::fs::{File, OpenOptions, create_dir_all};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Append-only action audit log: action + nickname (+ optional channel). Never stores message bodies.
#[derive(Clone)]
pub struct ActionLogger {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    dir: PathBuf,
    prefix: String,
    current_date: String,
    file: File,
}

impl ActionLogger {
    pub fn open(log_path: &Path) -> Result<Self> {
        let (dir, prefix) = resolve_dir_and_prefix(log_path);
        create_dir_all(&dir).with_context(|| format!("create log dir {}", dir.display()))?;
        let date = Local::now().format("%Y-%m-%d").to_string();
        let file = open_dated_file(&dir, &prefix, &date)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner {
                dir,
                prefix,
                current_date: date,
                file,
            })),
        })
    }

    /// Log `action` performed by `nickname`. Optional `channel` for join/message. Never logs content.
    pub fn record(&self, action: &str, nickname: &str, channel: Option<&str>) {
        let ts = Local::now().format("%Y-%m-%dT%H:%M:%S%z");
        let line = match channel {
            Some(ch) => format!(
                "{ts} action={action} nickname={nickname} channel={ch}\n"
            ),
            None => format!("{ts} action={action} nickname={nickname}\n"),
        };

        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        if let Err(err) = guard.write_line(&line) {
            tracing::warn!(error = %err, "action log write failed");
        }
    }
}

impl Inner {
    fn write_line(&mut self, line: &str) -> Result<()> {
        let date = Local::now().format("%Y-%m-%d").to_string();
        if date != self.current_date {
            self.file = open_dated_file(&self.dir, &self.prefix, &date)?;
            self.current_date = date;
        }
        self.file
            .write_all(line.as_bytes())
            .context("write action log")?;
        self.file.flush().context("flush action log")?;
        Ok(())
    }
}

fn resolve_dir_and_prefix(log_path: &Path) -> (PathBuf, String) {
    if log_path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("log"))
        || log_path.file_name().is_some_and(|n| n.to_string_lossy().contains('.'))
    {
        let dir = log_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = log_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "actions".into());
        (dir, stem)
    } else {
        (log_path.to_path_buf(), "actions".into())
    }
}

fn open_dated_file(dir: &Path, prefix: &str, date: &str) -> Result<File> {
    let path = dir.join(format!("{prefix}.{date}.log"));
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("open action log {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn records_action_without_message_body() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("agent_channel_action_log_{stamp}"));
        let log_path = dir.join("actions.log");

        let logger = ActionLogger::open(&log_path).expect("open");
        logger.record("message", "alice", Some("Group A"));
        logger.record("auth", "bob", None);

        let date = Local::now().format("%Y-%m-%d").to_string();
        let file_path = dir.join(format!("actions.{date}.log"));
        let body = fs::read_to_string(&file_path).expect("read");
        assert!(body.contains("action=message nickname=alice channel=Group A"));
        assert!(body.contains("action=auth nickname=bob"));
        assert!(!body.contains("secret payload"));

        let _ = fs::remove_dir_all(&dir);
    }
}

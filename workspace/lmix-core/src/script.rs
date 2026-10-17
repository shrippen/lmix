//! The show's script folder (show mode SSH.2): lmix-core keeps
//! `lmix-script load <folder> --watch` running for it, so the cue lists
//! follow the script after a restart or a show change without a terminal.
//! lmix-core itself still knows no Markdown (S-E.2): the tool talks to it
//! over the API like any client.
//!
//! ```text
//!   show.script_folder ──sync──► child: lmix-script load <folder> --watch
//!                                   └──HTTP──► lmix-core (set_cue_list …)
//! ```

use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// Keeps the tool running for one folder at a time.
pub struct ScriptRunner {
    tool: PathBuf,
    core_url: String,
    running: Option<(String, Child)>,
}

impl ScriptRunner {
    /// `tool`: the lmix-script binary; `core_url`: where it finds us.
    pub fn new(tool: PathBuf, core_url: String) -> Self {
        Self {
            tool,
            core_url,
            running: None,
        }
    }

    /// The folder it watches now, if the tool still runs.
    pub fn watching(&mut self) -> Option<&str> {
        let gone = match &mut self.running {
            Some((_, child)) => !matches!(child.try_wait(), Ok(None)),
            None => false,
        };
        if gone {
            self.running = None;
        }
        self.running.as_ref().map(|(folder, _)| folder.as_str())
    }

    /// Run the tool for `folder`, or none; a change stops the old one.
    pub async fn sync(&mut self, folder: Option<&str>) {
        if self.watching() == folder {
            return;
        }
        if let Some((old, mut child)) = self.running.take() {
            let _ = child.kill().await;
            tracing::info!(folder = %old, "script watch stopped");
        }
        let Some(folder) = folder else {
            return;
        };
        let spawned = Command::new(&self.tool)
            .arg("load")
            .arg(folder)
            .arg("--watch")
            .env("LMIX_CORE", &self.core_url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(e) => {
                tracing::warn!(error = %e, tool = %self.tool.display(), "script tool not started");
                return;
            }
        };
        // Its lines (loaded lists, problems with file and line) go to
        // the log.
        if let Some(out) = child.stdout.take() {
            tokio::spawn(forward(BufReader::new(out), false));
        }
        if let Some(err) = child.stderr.take() {
            tokio::spawn(forward(BufReader::new(err), true));
        }
        tracing::info!(%folder, "script watch started");
        self.running = Some((folder.to_string(), child));
    }
}

async fn forward<R: tokio::io::AsyncRead + Unpin>(read: BufReader<R>, problems: bool) {
    let mut lines = read.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if problems {
            tracing::warn!(target: "lmix_script", "{line}");
        } else {
            tracing::info!(target: "lmix_script", "{line}");
        }
    }
}

/// The tool next to lmix-core's own binary, else from `PATH`.
pub fn default_tool() -> PathBuf {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("lmix-script")))
        .filter(|p| p.is_file());
    beside.unwrap_or_else(|| PathBuf::from("lmix-script"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in tool that writes its arguments and stays.
    fn fake_tool(dir: &std::path::Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let tool = dir.join("lmix-script");
        let log = dir.join("calls");
        std::fs::write(
            &tool,
            format!(
                "#!/bin/sh\necho \"$@ $LMIX_CORE\" >> {}\nexec sleep 60\n",
                log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        tool
    }

    #[tokio::test]
    async fn one_tool_per_folder() {
        let dir = tempfile::tempdir().unwrap();
        let mut runner = ScriptRunner::new(fake_tool(dir.path()), "http://127.0.0.1:8200".into());
        let calls = || std::fs::read_to_string(dir.path().join("calls")).unwrap_or_default();
        let wait = || tokio::time::sleep(std::time::Duration::from_millis(300));

        runner.sync(Some("/drehbuch")).await;
        runner.sync(Some("/drehbuch")).await;
        wait().await;
        assert_eq!(
            calls(),
            "load /drehbuch --watch http://127.0.0.1:8200\n",
            "once"
        );
        assert_eq!(runner.watching(), Some("/drehbuch"));

        runner.sync(Some("/anderes")).await;
        wait().await;
        assert_eq!(calls().lines().count(), 2);
        assert_eq!(runner.watching(), Some("/anderes"));

        runner.sync(None).await;
        assert_eq!(runner.watching(), None);
    }
}

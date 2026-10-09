//! Recently used show files (B24): newest first, kept next to the
//! autosave, so every client offers the same list.

use std::path::{Path, PathBuf};

/// Entries kept.
const RECENT_MAX: usize = 10;
const RECENT_FILE: &str = "recent.json";

pub struct Recent {
    file: Option<PathBuf>,
    paths: Vec<String>,
}

impl Recent {
    /// List stored beside `autosave`; none without autosave (tests).
    pub fn load(autosave: Option<&Path>) -> Self {
        let file = autosave
            .and_then(Path::parent)
            .map(|dir| dir.join(RECENT_FILE));
        let paths = file
            .as_ref()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { file, paths }
    }

    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    /// Put `path` first; best effort on disk.
    pub async fn touch(&mut self, path: &str) {
        self.paths = pushed(&self.paths, path);
        let Some(file) = &self.file else {
            return;
        };
        let json = serde_json::to_vec_pretty(&self.paths).expect("list serializes");
        if let Err(e) = tokio::fs::write(file, json).await {
            tracing::warn!(error = %e, "recent shows not saved");
        }
    }
}

fn pushed(paths: &[String], path: &str) -> Vec<String> {
    let mut list = vec![path.to_string()];
    list.extend(paths.iter().filter(|p| *p != path).cloned());
    list.truncate(RECENT_MAX);
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_without_duplicates() {
        let list = pushed(&["a".into(), "b".into()], "b");

        assert_eq!(list, ["b", "a"]);
    }

    #[test]
    fn keeps_at_most_ten() {
        let long: Vec<String> = (0..RECENT_MAX).map(|i| i.to_string()).collect();

        let list = pushed(&long, "new");

        assert_eq!(list.len(), RECENT_MAX);
        assert_eq!(list[0], "new");
    }

    #[tokio::test]
    async fn survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let autosave = dir.path().join("autosave.lmix");

        Recent::load(Some(&autosave))
            .touch("/shows/sunday.lmix")
            .await;

        assert_eq!(
            Recent::load(Some(&autosave)).paths(),
            ["/shows/sunday.lmix"]
        );
    }
}

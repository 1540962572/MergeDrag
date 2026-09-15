use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Clone, Parser, Default)]
#[command(name = "mergedrag", about = "IDEA-style three-pane Git mergetool")]
pub struct LaunchArgs {
    /// $LOCAL from git mergetool
    #[arg(long)]
    pub local: Option<PathBuf>,
    /// $REMOTE from git mergetool
    #[arg(long)]
    pub remote: Option<PathBuf>,
    /// $BASE from git mergetool (optional when no common ancestor)
    #[arg(long)]
    pub base: Option<PathBuf>,
    /// $MERGED from git mergetool (save target)
    #[arg(long)]
    pub merged: Option<PathBuf>,
}

impl LaunchArgs {
    pub fn is_mergetool_launch(&self) -> bool {
        self.local.is_some() && self.remote.is_some() && self.merged.is_some()
    }
}

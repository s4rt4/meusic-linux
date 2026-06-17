//! Small shared helpers.

use std::path::PathBuf;

/// Path to a file in the app's config dir (~/.config/com.sarta.meusic.gtk/),
/// creating the directory. None if no config base is resolvable.
pub fn config_file(name: &str) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    let dir = base.join("com.sarta.meusic.gtk");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join(name))
}

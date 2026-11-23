//! The binary this machine's server runs. On Linux it is the server's own copy in `~/.octopus/bin`, linked from
//! `~/.local/bin` so the CLI is on the PATH; on macOS it is the main binary of the app whose login item runs it.

use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

#[cfg(target_os = "linux")]
use anyhow::Context;

/// The server's copy, while it is there and an `octopus` sits on the PATH at `~/.local/bin`: the link to the copy,
/// or a file of the user's own, which stays theirs.
#[cfg(target_os = "linux")]
pub fn installed(home: &Path) -> Option<PathBuf> {
    let copy = copy(home);
    (copy.is_file() && std::fs::symlink_metadata(link_path(home)).is_ok()).then_some(copy)
}

/// Puts `source` where this machine's server runs it from and links it onto the PATH, returning the copy. The copy
/// is replaced by a rename, so a server starting meanwhile runs the old binary or the new one, never a torn one.
#[cfg(target_os = "linux")]
pub fn put(home: &Path, source: &Path) -> anyhow::Result<PathBuf> {
    let copy = copy(home);
    if !same_file(source, &copy) {
        let directory = copy.parent().expect("the copy sits in a directory");
        std::fs::create_dir_all(directory).with_context(|| format!("create {}", directory.display()))?;
        let staged = directory.join(format!(".octopus.{}", std::process::id()));
        let placed = std::fs::copy(source, &staged)
            .with_context(|| format!("copy {} to {}", source.display(), staged.display()))
            .and_then(|_| std::fs::rename(&staged, &copy).with_context(|| format!("replace {}", copy.display())));
        if placed.is_err() {
            let _gone_already = std::fs::remove_file(&staged);
        }
        placed?;
    }
    let link = link_path(home);
    if std::fs::symlink_metadata(&link).is_err() {
        let directory = link.parent().expect("the link sits in a directory");
        std::fs::create_dir_all(directory).with_context(|| format!("create {}", directory.display()))?;
        std::os::unix::fs::symlink(&copy, &link)
            .with_context(|| format!("link {} to {}", link.display(), copy.display()))?;
    }
    Ok(copy)
}

#[cfg(target_os = "linux")]
fn copy(home: &Path) -> PathBuf {
    super::octopus_dir(home).join("bin").join("octopus")
}

#[cfg(target_os = "linux")]
fn link_path(home: &Path) -> PathBuf {
    home.join(".local").join("bin").join("octopus")
}

#[cfg(target_os = "linux")]
fn same_file(one: &Path, other: &Path) -> bool {
    matches!((one.canonicalize(), other.canonicalize()), (Ok(one), Ok(other)) if one == other)
}

/// A program on a mounted disk image, in the read-only copy macOS translocates a downloaded app to, or in the Trash
/// is gone once the image is ejected, the app is moved or the Trash is emptied.
#[cfg(target_os = "macos")]
pub fn transient(program: &Path) -> bool {
    program.starts_with("/Volumes")
        || program.components().any(|part| matches!(part.as_os_str().to_str(), Some(".Trash" | "AppTranslocation")))
}

/// The app bundle `program` is the main binary of, `<name>.app/Contents/MacOS/octopus`.
#[cfg(target_os = "macos")]
pub fn bundle(program: &Path) -> Option<&Path> {
    let macos = program.parent().filter(|macos| macos.ends_with("Contents/MacOS"))?;
    macos.parent()?.parent().filter(|bundle| bundle.extension().is_some_and(|extension| extension == "app"))
}

/// What this launch runs as. A build without the `release` feature, which only the release workflow turns on, is a
/// build from source and stays the build it is.
#[cfg(feature = "desktop")]
pub fn app() -> octopus_desktop::App {
    if !cfg!(feature = "release") {
        return octopus_desktop::App::Source;
    }
    platform_app()
}

#[cfg(all(target_os = "macos", feature = "desktop"))]
fn platform_app() -> octopus_desktop::App {
    let program = std::env::current_exe().ok();
    match program.as_deref().and_then(bundle).map(Path::to_path_buf) {
        None => octopus_desktop::App::Source,
        Some(bundle) if transient(&bundle) => octopus_desktop::App::Transient(bundle),
        Some(bundle) => octopus_desktop::App::Installed(bundle),
    }
}

/// The AppImage this binary runs from, which its runtime names in APPIMAGE.
#[cfg(all(target_os = "linux", feature = "desktop"))]
fn platform_app() -> octopus_desktop::App {
    std::env::var_os("APPIMAGE")
        .map_or(octopus_desktop::App::Source, |file| octopus_desktop::App::Installed(file.into()))
}

/// The binary this launch installs as the server's: within an AppImage, the headless build it carries at
/// `usr/lib/octopus/octopus` under the directory its runtime names in APPDIR, which runs without the window libraries
/// only the AppImage holds; else this binary.
#[cfg(target_os = "linux")]
pub fn this() -> std::io::Result<PathBuf> {
    match std::env::var_os("APPDIR") {
        Some(appdir) => Ok(Path::new(&appdir).join("usr/lib/octopus/octopus")),
        None => std::env::current_exe(),
    }
}

#[cfg(target_os = "macos")]
pub fn this() -> std::io::Result<std::path::PathBuf> {
    std::env::current_exe()
}

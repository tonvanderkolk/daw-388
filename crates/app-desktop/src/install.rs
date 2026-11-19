//! Installing a new version on macOS: download the DMG of the release, copy the app out of it next
//! to this one, check that Apple notarized it for this developer and that it is the version asked
//! for, then swap it in. The old app goes to the Trash. Anything that fails leaves this app as it was.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

/// The Apple Developer team that signs DAW 388: a new version must be signed by it.
const TEAM_ID: &str = "HS386KSWR7";
/// Where the releases are, as `bundle.sh` and build.yml name the DMG.
fn dmg_url(version: &str) -> String {
    format!("https://github.com/tonvanderkolk/daw-388/releases/download/v{version}/DAW-388-{version}-macos.dmg")
}

/// The app bundle this program runs from; `None` for a build outside a bundle.
pub fn this_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf)
}

/// Whether this app can replace itself: it runs from a bundle in a folder it may write in.
pub fn can_install() -> bool {
    this_bundle().and_then(|b| b.parent().map(Path::to_path_buf)).is_some_and(|dir| {
        let probe = dir.join(".daw388-write-test");
        let ok = std::fs::File::create(&probe).is_ok();
        let _ = std::fs::remove_file(&probe);
        ok
    })
}

fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<String, String> {
    let output = Command::new(program).args(args).output().map_err(|e| format!("{program}: {e}"))?;
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    if output.status.success() { Ok(text) } else { Err(format!("{program}: {}", text.trim())) }
}

/// Download `url` to `to`, counting from 0 to 900 of `progress`.
fn download(url: &str, to: &Path, progress: &AtomicU32) -> Result<(), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_connect(Some(std::time::Duration::from_secs(15))).build().into();
    let mut response =
        agent.get(url).header("User-Agent", concat!("DAW388/", env!("CARGO_PKG_VERSION"))).call().map_err(|e| format!("download: {e}"))?;
    let total: u64 = response.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut reader = response.body_mut().with_config().limit(500 * 1024 * 1024).reader();
    let mut file = std::fs::File::create(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let mut buffer = vec![0u8; 256 * 1024];
    let mut done: u64 = 0;
    loop {
        let n = reader.read(&mut buffer).map_err(|e| format!("download: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n]).map_err(|e| format!("{}: {e}", to.display()))?;
        done += n as u64;
        if let Some(part) = (done * 900).checked_div(total) {
            progress.store(part.min(900) as u32, Ordering::Relaxed);
        }
    }
    if total > 0 && done != total {
        return Err("the download stopped early".into());
    }
    Ok(())
}

/// Signed by [`TEAM_ID`], notarized by Apple, and the version asked for.
fn check(app: &Path, version: &str) -> Result<(), String> {
    let path = app.as_os_str();
    run("codesign", &["--verify".as_ref(), "--deep".as_ref(), "--strict".as_ref(), path])
        .map_err(|_| "the new version's signature is not valid".to_string())?;
    let details = run("codesign", &["-dv".as_ref(), path])?;
    if !details.lines().any(|l| l.trim() == format!("TeamIdentifier={TEAM_ID}")) {
        return Err("the new version is not signed by the maker of DAW 388".into());
    }
    let assessment = run("spctl", &["--assess".as_ref(), "--type".as_ref(), "execute".as_ref(), "-vv".as_ref(), path])
        .map_err(|_| "macOS does not accept the new version".to_string())?;
    if !assessment.contains("Notarized Developer ID") {
        return Err("the new version is not notarized by Apple".into());
    }
    let plist = app.join("Contents/Info.plist");
    let found = run("plutil", &["-extract".as_ref(), "CFBundleShortVersionString".as_ref(), "raw".as_ref(), plist.as_os_str()])?;
    if found.trim() != version {
        return Err(format!("the download is version {}, not {version}", found.trim()));
    }
    Ok(())
}

/// Download, check and install `version` in place of this app; gives the path of the new app.
pub fn install(version: &str, progress: &AtomicU32, trash: impl Fn(&Path) -> Result<(), String>) -> Result<PathBuf, String> {
    let bundle = this_bundle().ok_or("DAW 388 does not run from an app")?;
    let dir = bundle.parent().ok_or("no folder around the app")?.to_path_buf();
    let work = std::env::temp_dir().join(format!("daw388-update-{version}"));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let result = (|| {
        let dmg = work.join("update.dmg");
        download(&dmg_url(version), &dmg, progress)?;
        let mount = work.join("mount");
        run(
            "hdiutil",
            &[
                "attach".as_ref(),
                "-nobrowse".as_ref(),
                "-readonly".as_ref(),
                "-noautoopen".as_ref(),
                "-mountpoint".as_ref(),
                mount.as_os_str(),
                dmg.as_os_str(),
            ],
        )?;
        // Copy the app next to this one (the same disk, so the swap is a rename), then let go of the DMG.
        let staged = dir.join(format!(".DAW 388 {version}.app"));
        let _ = std::fs::remove_dir_all(&staged);
        let copied = std::fs::read_dir(&mount)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .ok_or_else(|| "no app in the download".to_string())
            .and_then(|app| run("ditto", &[app.as_os_str(), staged.as_os_str()]));
        let _ = run("hdiutil", &["detach".as_ref(), "-force".as_ref(), mount.as_os_str()]);
        copied?;
        progress.store(950, Ordering::Relaxed);
        if let Err(e) = check(&staged, version) {
            let _ = std::fs::remove_dir_all(&staged);
            return Err(e);
        }
        // The swap: this app aside, the new one in its place, then the old one to the Trash.
        let aside = dir.join(format!(".DAW 388 old {}.app", env!("CARGO_PKG_VERSION")));
        let _ = std::fs::remove_dir_all(&aside);
        std::fs::rename(&bundle, &aside).map_err(|e| format!("cannot move this app aside: {e}"))?;
        if let Err(e) = std::fs::rename(&staged, &bundle) {
            let _ = std::fs::rename(&aside, &bundle);
            let _ = std::fs::remove_dir_all(&staged);
            return Err(format!("cannot put the new version in place: {e}"));
        }
        if let Err(e) = trash(&aside) {
            log::warn!("the old version stays at {}: {e}", aside.display());
        }
        progress.store(1000, Ordering::Relaxed);
        Ok(bundle.clone())
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// Start the app at `path` as a new process (the new version, once this one has quit).
pub fn launch(path: &Path) {
    if let Err(e) = Command::new("open").arg("-n").arg(path).spawn() {
        log::warn!("cannot start {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dmg_url_follows_the_release_names() {
        assert_eq!(dmg_url("0.39.0"), "https://github.com/tonvanderkolk/daw-388/releases/download/v0.39.0/DAW-388-0.39.0-macos.dmg");
    }

    #[test]
    fn a_test_build_outside_an_app_cannot_install() {
        assert!(this_bundle().is_none(), "cargo test runs outside an app bundle");
        assert!(!can_install());
    }
}

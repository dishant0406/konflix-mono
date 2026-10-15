//! Installing a downloaded update.
//!
//! macOS: the disk image is mounted, the app inside must pass `codesign` and carry our Apple
//! Team ID (that signature is what is trusted; the hash only guards against corruption), and
//! then it replaces the running bundle, with the old one put back if anything goes wrong.
//! Windows: the installer is run only if it carries a valid signature from SignPath
//! Foundation; until it does, the release page is opened instead.
//!
//! Anything that cannot be installed safely (the app is not in a bundle, its folder is not
//! writable, the installer is unsigned) ends as `Installed::OpenPage`, never as a guess.

use akidev_core::update::{RELEASES_PAGE, Update};
use anyhow::{Context, ensure};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The Apple Developer ID team the macOS app is signed by.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const APPLE_TEAM_ID: &str = "689N5CNAYJ";

/// Test hooks: pretend to run this version, and replace this `.app` instead of the running one.
const CURRENT_VERSION_VAR: &str = "AKIDEV_UPDATE_CURRENT";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const BUNDLE_VAR: &str = "AKIDEV_UPDATE_BUNDLE";

/// What happened to a downloaded update.
#[derive(Debug, PartialEq, Eq)]
pub enum Installed {
    /// The new version is in place and starting; the app should quit now.
    Restart,
    /// It was not installed, for this reason; the viewer is sent to the download page.
    OpenPage(String),
}

/// The version to compare releases against.
pub fn current_version() -> String {
    std::env::var(CURRENT_VERSION_VAR).unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

/// The page to open when the update cannot be applied here. Only ever a GitHub page.
pub fn download_page(update: Option<&Update>) -> String {
    update
        .map(|update| update.release_url.as_str())
        .filter(|url| akidev_core::update::is_allowed_address(url))
        .unwrap_or(RELEASES_PAGE)
        .to_string()
}

/// Whether an update could be installed here at all, asked before anything is downloaded.
/// `Err` carries the reason it cannot, in words for the viewer.
pub fn preflight() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        mac::target_bundle()
            .map(|_| ())
            .map_err(|error| format!("{error:#}"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

/// Installs the downloaded file. Blocking; run it off the UI thread. Returns once the new
/// version has been started (macOS) or the installer launched (Windows).
pub fn install(downloaded: &Path, update: &Update) -> anyhow::Result<Installed> {
    #[cfg(target_os = "macos")]
    {
        let _ = update;
        mac::install(downloaded)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = update;
        windows::install(downloaded)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (downloaded, update);
        Ok(Installed::OpenPage(
            "Updates are not installed on this system.".into(),
        ))
    }
}

/// The `.app` that contains the executable at `exe`: `X.app/Contents/MacOS/<exe>`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app")
        .then(|| bundle.to_path_buf())
}

/// The value of `TeamIdentifier=` in `codesign -dv` output.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn team_identifier(output: &str) -> Option<&str> {
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix("TeamIdentifier="))
        .map(str::trim)
}

/// What to do with a downloaded Windows installer, from its Authenticode status and the
/// subject of the certificate that signed it.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
enum Trust {
    Run,
    OpenPage(String),
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn windows_trust(status: &str, subject: &str) -> Trust {
    if status.trim() == "Valid" && subject.contains("SignPath Foundation") {
        Trust::Run
    } else if status.trim() == "Valid" {
        Trust::OpenPage("The installer is signed by someone other than Akidev's publisher.".into())
    } else {
        Trust::OpenPage("The installer is not signed yet, so it is not run from here.".into())
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use std::os::unix::process::CommandExt as _;

    /// The bundle to replace, or why it cannot be.
    pub fn target_bundle() -> anyhow::Result<PathBuf> {
        let bundle = match std::env::var_os(BUNDLE_VAR) {
            Some(path) => PathBuf::from(path),
            None => {
                let exe = std::env::current_exe().context("finding the running app")?;
                bundle_of(&exe).context("Akidev is not running from an app bundle")?
            }
        };
        ensure!(bundle.is_dir(), "{} does not exist", bundle.display());
        let parent = bundle.parent().context("the app has no folder")?;
        // Writing is the only honest test: a translocated or read-only volume refuses it.
        let probe = parent.join(format!(".akidev-write-test-{}", std::process::id()));
        std::fs::write(&probe, b"")
            .and_then(|()| std::fs::remove_file(&probe))
            .map_err(|_| anyhow::anyhow!("Akidev cannot change the folder it is installed in"))?;
        Ok(bundle)
    }

    fn run(program: &str, args: &[&std::ffi::OsStr]) -> anyhow::Result<std::process::Output> {
        let output = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .with_context(|| format!("running {program}"))?;
        if !output.status.success() {
            let said = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("{program} failed: {}", said.trim());
        }
        Ok(output)
    }

    /// A mounted disk image that is detached, and its mount folder removed, when dropped.
    struct Mount(PathBuf);

    impl Mount {
        fn attach(dmg: &Path) -> anyhow::Result<Mount> {
            let at =
                std::env::temp_dir().join(format!("akidev-update-mount-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&at);
            std::fs::create_dir_all(&at)?;
            let mount = Mount(at);
            run(
                "hdiutil",
                &[
                    "attach".as_ref(),
                    "-nobrowse".as_ref(),
                    "-readonly".as_ref(),
                    "-mountpoint".as_ref(),
                    mount.0.as_os_str(),
                    dmg.as_os_str(),
                ],
            )
            .context("opening the disk image")?;
            Ok(mount)
        }
    }

    impl Drop for Mount {
        fn drop(&mut self) {
            if run("hdiutil", &["detach".as_ref(), self.0.as_os_str()]).is_err() {
                let _ = run(
                    "hdiutil",
                    &["detach".as_ref(), "-force".as_ref(), self.0.as_os_str()],
                );
            }
            let _ = std::fs::remove_dir(&self.0);
        }
    }

    /// The signature check that decides whether an app is ours.
    fn verify_signature(app: &Path) -> anyhow::Result<()> {
        run(
            "codesign",
            &[
                "--verify".as_ref(),
                "--deep".as_ref(),
                "--strict".as_ref(),
                app.as_os_str(),
            ],
        )
        .context("the app's signature is not valid")?;
        let output = run(
            "codesign",
            &["-dv".as_ref(), "--verbose=4".as_ref(), app.as_os_str()],
        )?;
        // codesign describes the signature on stderr.
        let said = String::from_utf8_lossy(&output.stderr);
        let team = team_identifier(&said);
        ensure!(
            team == Some(APPLE_TEAM_ID),
            "the app is not signed by Akidev's Apple team (found {})",
            team.unwrap_or("none")
        );
        Ok(())
    }

    fn sibling(bundle: &Path, suffix: &str) -> PathBuf {
        let mut name = bundle.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    }

    pub fn install(dmg: &Path) -> anyhow::Result<Installed> {
        let bundle = match target_bundle() {
            Ok(bundle) => bundle,
            Err(error) => return Ok(Installed::OpenPage(format!("{error:#}"))),
        };
        let new = sibling(&bundle, ".update");
        let old = sibling(&bundle, ".old");
        let swapped = (|| {
            let mount = Mount::attach(dmg)?;
            let app = mount.0.join("Akidev.app");
            ensure!(app.is_dir(), "the disk image has no Akidev.app");
            verify_signature(&app)?;
            let _ = std::fs::remove_dir_all(&new);
            let _ = std::fs::remove_dir_all(&old);
            run("ditto", &[app.as_os_str(), new.as_os_str()]).context("copying the app")?;
            // What was copied is checked again, since that is what will run.
            verify_signature(&new)?;
            swap(&bundle, &new, &old)
            // `mount` is detached here, after the copy is made.
        })();
        if let Err(error) = swapped {
            let _ = std::fs::remove_dir_all(&new);
            return Err(error);
        }
        let _ = std::fs::remove_dir_all(&old);
        let _ = std::fs::remove_file(dmg);
        relaunch(&bundle)?;
        Ok(Installed::Restart)
    }

    /// Puts `new` where `bundle` is, keeping the old one aside, and puts the old one back if
    /// the result does not check out.
    fn swap(bundle: &Path, new: &Path, old: &Path) -> anyhow::Result<()> {
        std::fs::rename(bundle, old).context("moving the old app aside")?;
        let placed = std::fs::rename(new, bundle)
            .context("putting the new app in place")
            .and_then(|()| verify_signature(bundle));
        if let Err(error) = placed {
            let _ = std::fs::remove_dir_all(bundle);
            std::fs::rename(old, bundle).context("restoring the old app")?;
            return Err(error);
        }
        Ok(())
    }

    /// Starts the new app a moment from now, from a process that outlives this one.
    fn relaunch(bundle: &Path) -> anyhow::Result<()> {
        // The path is an argument of the script, not part of it, so no name can alter it.
        Command::new("sh")
            .args(["-c", "sleep 1; open -n \"$1\"", "sh"])
            .arg(bundle)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .context("restarting Akidev")?;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::os::windows::process::CommandExt as _;

    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// The Authenticode status and the signing certificate's subject.
    fn signature(exe: &Path) -> anyhow::Result<(String, String)> {
        let quoted = exe.to_string_lossy().replace('\'', "''");
        let script = format!(
            "$s = Get-AuthenticodeSignature -LiteralPath '{quoted}'; \
             $s.Status; $s.SignerCertificate.Subject"
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .stdin(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .context("checking the installer's signature")?;
        ensure!(
            output.status.success(),
            "checking the installer's signature failed"
        );
        let text = String::from_utf8_lossy(&output.stdout);
        let mut lines = text.lines();
        Ok((
            lines.next().unwrap_or_default().to_string(),
            lines.next().unwrap_or_default().to_string(),
        ))
    }

    pub fn install(installer: &Path) -> anyhow::Result<Installed> {
        let (status, subject) = signature(installer)?;
        match windows_trust(&status, &subject) {
            Trust::OpenPage(reason) => Ok(Installed::OpenPage(reason)),
            Trust::Run => {
                // The installer closes this app, installs per user and starts it again.
                Command::new(installer)
                    .args([
                        "/VERYSILENT",
                        "/SUPPRESSMSGBOXES",
                        "/NORESTART",
                        "/CLOSEAPPLICATIONS",
                        "/RESTARTAPPLICATIONS",
                    ])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
                    .spawn()
                    .context("starting the installer")?;
                Ok(Installed::Restart)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundle_is_found_from_the_executable_inside_it() {
        assert_eq!(
            bundle_of(Path::new("/Applications/Akidev.app/Contents/MacOS/Akidev")),
            Some(PathBuf::from("/Applications/Akidev.app"))
        );
        assert_eq!(bundle_of(Path::new("/work/target/release/akidev")), None);
        assert_eq!(
            bundle_of(Path::new("/x/Folder/Contents/MacOS/Akidev")),
            None
        );
        assert_eq!(bundle_of(Path::new("Akidev")), None);
    }

    #[test]
    fn the_team_is_read_from_codesign_output() {
        let output = "Executable=/x/Akidev\nIdentifier=live.akidev.Akidev\nTeamIdentifier=689N5CNAYJ\nSealed Resources=none\n";
        assert_eq!(team_identifier(output), Some("689N5CNAYJ"));
        assert_eq!(team_identifier("TeamIdentifier=not set\n"), Some("not set"));
        assert_eq!(team_identifier("Identifier=x\n"), None);
    }

    #[test]
    fn only_a_valid_signature_from_signpath_is_run() {
        let subject = "CN=SignPath Foundation, O=SignPath Foundation, L=Lewes, S=Delaware, C=US";
        assert_eq!(windows_trust("Valid", subject), Trust::Run);
        assert_eq!(windows_trust("Valid\r", subject), Trust::Run);
        assert!(matches!(windows_trust("NotSigned", ""), Trust::OpenPage(_)));
        assert!(matches!(
            windows_trust("HashMismatch", subject),
            Trust::OpenPage(_)
        ));
        assert!(matches!(
            windows_trust("UnknownError", subject),
            Trust::OpenPage(_)
        ));
        assert!(matches!(
            windows_trust("Valid", "CN=Somebody Else"),
            Trust::OpenPage(_)
        ));
        assert!(matches!(windows_trust("", ""), Trust::OpenPage(_)));
    }

    #[test]
    fn only_github_pages_are_opened() {
        let mut update = Update {
            version: "0.1.4".into(),
            tag: String::new(),
            asset_name: String::new(),
            asset_url: String::new(),
            sha256_url: String::new(),
            size: 0,
            release_url: "https://github.com/a/b/releases/tag/t".into(),
            notes: String::new(),
        };
        assert_eq!(download_page(Some(&update)), update.release_url);
        update.release_url = "https://evil.example/".into();
        assert_eq!(download_page(Some(&update)), RELEASES_PAGE);
        assert_eq!(download_page(None), RELEASES_PAGE);
    }
}

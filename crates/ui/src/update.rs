//! Checking GitHub for a newer release: at startup at most once a day (Settings), or from the
//! Help menu. On macOS a signed app installs it itself (Help, Install DAW 388 ...): the platform
//! downloads the DMG, checks that Apple notarized it for this developer, and puts it in place;
//! elsewhere the app links to the release page.

use super::*;

/// The latest release, as GitHub's API gives it.
pub(crate) const RELEASES_API: &str = "https://api.github.com/repos/tonvanderkolk/daw-388/releases/latest";
/// The page with the latest release, for the user to download it.
pub(crate) const RELEASES_PAGE: &str = "https://github.com/tonvanderkolk/daw-388/releases/latest";
/// How long the answer of a check holds, in seconds: one check a day at most.
const CHECK_EVERY: u64 = 24 * 60 * 60;

/// "v0.31.0", "0.31" or "0.31.0-beta" as numbers; `None` for anything else.
pub(crate) fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.trim().trim_start_matches('v').split('.').map(|part| {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u32>().ok()
    });
    let major = parts.next()??;
    let minor = parts.next()??;
    let patch = parts.next().flatten().unwrap_or(0);
    Some((major, minor, patch))
}

/// Whether `latest` is a newer version than `current`.
pub(crate) fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse_version(latest), parse_version(current)), (Some(l), Some(c)) if l > c)
}

/// The version in the answer of the releases API ("tag_name": "v0.31.0"), without the v.
pub(crate) fn release_version(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let tag = value.get("tag_name")?.as_str()?;
    parse_version(tag)?;
    Some(tag.trim().trim_start_matches('v').to_string())
}

/// Seconds since 1970, for when the last check was.
fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// This app's own version.
pub(crate) fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

impl DawApp {
    /// At startup: what the last check found, and a new check when a day has passed.
    pub(crate) fn startup_update_check(&mut self) {
        if !self.preferences.check_updates {
            return;
        }
        if let Some(latest) = self.preferences.latest_version.clone()
            && is_newer(&latest, current_version())
        {
            self.update_available = Some(latest);
        }
        let due = self.preferences.update_checked_at.is_none_or(|at| now_unix().saturating_sub(at) >= CHECK_EVERY);
        if due {
            self.check_for_updates(false);
        }
    }

    /// Ask GitHub for the latest release on a background thread. `manual` (Help, Check for
    /// Updates...) also reports "up to date" and errors; the check at startup stays quiet then.
    pub(crate) fn check_for_updates(&mut self, manual: bool) {
        let Some(fetch) = self.platform.fetcher() else {
            if manual {
                self.status = "This build cannot check for updates".to_string();
            }
            return;
        };
        let (tx, rx) = channel();
        self.update_rx = Some(rx);
        self.update_manual = manual;
        if manual {
            self.status = "Checking for updates...".to_string();
        }
        self.platform.spawn(Box::new(move || {
            let answer = fetch(RELEASES_API).and_then(|body| release_version(&body).ok_or_else(|| "no release found".to_string()));
            let _ = tx.send(answer);
        }));
    }

    /// Take the answer of a check when it is there.
    pub(crate) fn poll_update(&mut self) {
        let Some(rx) = &self.update_rx else { return };
        let Ok(answer) = rx.try_recv() else { return };
        self.update_rx = None;
        match answer {
            Ok(latest) => {
                self.preferences.update_checked_at = Some(now_unix());
                self.preferences.latest_version = Some(latest.clone());
                self.store_preferences();
                if is_newer(&latest, current_version()) {
                    let item = if self.platform.installer().is_some() { "Install" } else { "Download" };
                    self.status = format!("DAW 388 {latest} is available: Help, {item} DAW 388 {latest}...");
                    self.update_available = Some(latest);
                } else {
                    self.update_available = None;
                    if self.update_manual {
                        self.status = format!("DAW 388 {} is the latest version", current_version());
                    }
                }
            }
            Err(e) => {
                log::info!("update check failed: {e}");
                if self.update_manual {
                    self.status = format!("Cannot check for updates: {e}");
                }
            }
        }
    }

    /// Download, check and install `version` on a background thread.
    pub(crate) fn install_update(&mut self, version: String) {
        let Some(install) = self.platform.installer() else {
            self.platform.open_url(RELEASES_PAGE);
            return;
        };
        if self.install_rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        self.install_rx = Some(rx);
        self.install_progress.store(0, std::sync::atomic::Ordering::Relaxed);
        let progress = Arc::clone(&self.install_progress);
        let wanted = version.clone();
        self.installing = Some(version);
        self.platform.spawn(Box::new(move || {
            let _ = tx.send(install(&wanted, &progress));
        }));
    }

    /// Show how far an install is, and take its answer when it is there.
    pub(crate) fn poll_install(&mut self) {
        if self.restarting && self.quit_answer() == Some(false) {
            self.restarting = false;
            self.platform.relaunch_on_exit(None);
        }
        let Some(rx) = &self.install_rx else { return };
        let version = self.installing.clone().unwrap_or_default();
        match rx.try_recv() {
            Ok(Ok(app)) => {
                self.install_rx = None;
                self.installing = None;
                self.update_available = None;
                self.status = format!("DAW 388 {version} is installed");
                self.installed = Some((version, app));
            }
            Ok(Err(e)) => {
                self.install_rx = None;
                self.installing = None;
                log::warn!("install of {version} failed: {e}");
                self.status = format!("Cannot install DAW 388 {version}: {e}. Help, Download... opens the release page");
            }
            Err(_) => {
                let done = self.install_progress.load(std::sync::atomic::Ordering::Relaxed);
                self.status = if done < 900 {
                    format!("Downloading DAW 388 {version}: {}%", done / 9)
                } else {
                    format!("Checking and installing DAW 388 {version}...")
                };
            }
        }
    }

    /// The new version is in place: restart now (asking about unsaved work first) or later.
    pub(crate) fn installed_dialog(&mut self, ctx: &egui::Context) {
        let Some((version, app)) = self.installed.clone() else { return };
        let mut answer = None;
        let title = format!("DAW 388 {version} is installed");
        let response = skin::paper::paper_modal(ctx, "update-installed", &title, 400.0, |ui| {
            ui.label("Restart to use the new version. If there are unsaved changes, DAW 388 asks about them first.");
            ui.add_space(14.0);
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 34.0), Layout::right_to_left(Align::Center), |ui| {
                if skin::paper::primary_button(ui, "Restart").clicked() {
                    answer = Some(true);
                }
                if skin::paper::plain_button(ui, "Later").clicked() {
                    answer = Some(false);
                }
            });
        });
        if answer == Some(true) {
            self.installed = None;
            self.restarting = true;
            self.platform.relaunch_on_exit(Some(app));
            self.request_quit();
        } else if answer == Some(false) || response.should_close() {
            self.installed = None;
            self.status = format!("DAW 388 {version} starts the next time you open it");
        }
    }

    /// The update items of the Help menu.
    pub(crate) fn update_menu_items(&mut self, ui: &mut Ui) {
        if let Some(latest) = self.update_available.clone() {
            if self.platform.installer().is_some() {
                let busy = self.install_rx.is_some();
                if ui
                    .add_enabled(!busy, egui::Button::new(format!("Install DAW 388 {latest}...")))
                    .on_hover_text("Download it, check it and put it in place of this version")
                    .clicked()
                {
                    self.install_update(latest.clone());
                }
            }
            if ui.button(format!("Download DAW 388 {latest}...")).on_hover_text(RELEASES_PAGE).clicked() {
                self.platform.open_url(RELEASES_PAGE);
            }
        }
        if ui.button("Check for Updates...").clicked() {
            self.check_for_updates(true);
        }
    }

    /// The Tooltips part of Settings.
    pub(crate) fn tooltip_settings(&mut self, ui: &mut Ui) {
        skin::paper::section_title(ui, "Tooltips");
        let mut show = self.preferences.tooltips;
        skin::paper::check(ui, &mut show, "Explain controls when the mouse rests on them");
        if show != self.preferences.tooltips {
            self.preferences.tooltips = show;
            self.store_preferences();
        }
    }

    /// The Updates part of Settings.
    pub(crate) fn update_settings(&mut self, ui: &mut Ui) {
        skin::paper::section_title(ui, "Updates");
        let mut check = self.preferences.check_updates;
        skin::paper::check(ui, &mut check, "Check for updates at startup (at most once a day)");
        if check != self.preferences.check_updates {
            self.preferences.check_updates = check;
            self.store_preferences();
        }
        skin::paper::note(
            ui,
            "DAW 388 asks GitHub which version is the latest; nothing else is sent. A new version is only \
             announced: you download it yourself from the release page.",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers() {
        assert_eq!(parse_version("v0.31.0"), Some((0, 31, 0)));
        assert_eq!(parse_version("0.31"), Some((0, 31, 0)));
        assert_eq!(parse_version("1.2.3-beta"), Some((1, 2, 3)));
        assert_eq!(parse_version("nightly"), None);
        assert!(is_newer("v0.31.0", "0.30.0"));
        assert!(is_newer("0.100.0", "0.99.9"), "numbers, not text");
        assert!(!is_newer("v0.30.0", "0.30.0"));
        assert!(!is_newer("0.29.5", "0.30.0"));
        assert!(!is_newer("garbage", "0.30.0"));
    }

    #[test]
    fn the_version_comes_from_the_tag() {
        assert_eq!(release_version(r#"{"tag_name": "v0.31.0", "name": "x"}"#).as_deref(), Some("0.31.0"));
        assert_eq!(release_version(r#"{"message": "Not Found"}"#), None);
        assert_eq!(release_version("not json"), None);
    }
}

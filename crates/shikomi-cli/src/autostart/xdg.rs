//! Linux XDG Autostart フォールバックバックエンド（Sub-B Issue #127）。
//!
//! 設計根拠: docs/features/daemon-default-mode/autostart/detailed-design/xdg.md

use std::path::PathBuf;

use super::{resolve_daemon_path, AutostartBackend, AutostartError};

const DESKTOP_TEMPLATE: &str = "[Desktop Entry]
Type=Application
Name=shikomi-daemon
Comment=shikomi credential vault daemon
Exec={daemon_path}
Hidden=false
NoDisplay=false
X-GNOME-Autostart-enabled=true
";

pub struct XdgAutostartBackend;

impl XdgAutostartBackend {
    fn desktop_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".config/autostart/shikomi-daemon.desktop"))
    }
}

impl AutostartBackend for XdgAutostartBackend {
    fn name(&self) -> &'static str {
        "XdgAutostartBackend"
    }

    fn install(&self) -> Result<(), AutostartError> {
        let daemon_path = resolve_daemon_path()?;
        let daemon_path_str = daemon_path.display().to_string();

        let content = DESKTOP_TEMPLATE.replace("{daemon_path}", &daemon_path_str);

        let autostart_dir = dirs::home_dir()
            .ok_or_else(|| {
                AutostartError::IoError(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "cannot determine home directory",
                ))
            })?
            .join(".config/autostart");
        std::fs::create_dir_all(&autostart_dir)?;

        let desktop_path = Self::desktop_path().ok_or_else(|| {
            AutostartError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "cannot determine home directory",
            ))
        })?;
        std::fs::write(&desktop_path, content)?;
        Ok(())
    }

    fn uninstall(&self) -> Result<(), AutostartError> {
        if let Some(desktop) = Self::desktop_path() {
            match std::fs::remove_file(&desktop) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(AutostartError::IoError(e)),
            }
        }
        Ok(())
    }

    fn is_registered(&self) -> bool {
        Self::desktop_path().map(|p| p.exists()).unwrap_or(false)
    }

    fn install_hint(&self) -> Option<String> {
        Some("hint: this uses XDG Autostart; shikomi-daemon will start on next login".to_owned())
    }
}

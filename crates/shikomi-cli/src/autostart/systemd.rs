//! Linux systemd user unit バックエンド（Sub-B Issue #127）。
//!
//! 設計根拠: docs/features/daemon-default-mode/autostart/detailed-design/systemd.md

use std::path::PathBuf;

use super::{resolve_daemon_path, truncate_stderr, AutostartBackend, AutostartError};

const UNIT_TEMPLATE: &str = "[Unit]
Description=shikomi credential vault daemon
After=default.target

[Service]
ExecStart={daemon_path}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
";

pub struct SystemdBackend;

impl SystemdBackend {
    fn unit_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".config/systemd/user/shikomi-daemon.service"))
    }

    fn run_systemctl(args: &[&str]) -> Result<std::process::Output, AutostartError> {
        let out = std::process::Command::new("systemctl")
            .args(args)
            .output()?;
        Ok(out)
    }

    /// systemd user session が利用可能かを判定する。
    ///
    /// 設計根拠: systemd.md §SystemdBackend::is_available() 判定ロジック
    pub fn is_available() -> bool {
        // 1. PATH 上に systemctl が存在するか
        if !is_on_path("systemctl") {
            return false;
        }
        // 2. D-Bus セッションバスが存在するか
        if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_err() {
            return false;
        }
        // 3. systemctl --user status の exit code が 4 以外か
        match std::process::Command::new("systemctl")
            .args(["--user", "status", "--no-pager"])
            .output()
        {
            Ok(out) => {
                let code = out.status.code().unwrap_or(1);
                code != 4
            }
            Err(_) => false,
        }
    }
}

impl AutostartBackend for SystemdBackend {
    fn name(&self) -> &'static str {
        "SystemdBackend"
    }

    fn install(&self) -> Result<(), AutostartError> {
        let daemon_path = resolve_daemon_path()?;
        let daemon_path_str = daemon_path.display().to_string();

        let unit_content = UNIT_TEMPLATE.replace("{daemon_path}", &daemon_path_str);

        let unit_dir = dirs::home_dir()
            .ok_or_else(|| {
                AutostartError::IoError(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "cannot determine home directory",
                ))
            })?
            .join(".config/systemd/user");
        std::fs::create_dir_all(&unit_dir)?;

        let unit_path = Self::unit_path().ok_or_else(|| {
            AutostartError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "cannot determine home directory",
            ))
        })?;
        std::fs::write(&unit_path, unit_content)?;

        // daemon-reload
        let reload = Self::run_systemctl(&["--user", "daemon-reload"])?;
        if !reload.status.success() {
            return Err(AutostartError::CommandFailed {
                cmd: "systemctl --user daemon-reload".to_owned(),
                stderr_excerpt: truncate_stderr(&reload.stderr),
            });
        }

        // enable --now（登録 + 即時起動）
        let enable = Self::run_systemctl(&["--user", "enable", "--now", "shikomi-daemon.service"])?;
        if !enable.status.success() {
            return Err(AutostartError::CommandFailed {
                cmd: "systemctl --user enable --now shikomi-daemon.service".to_owned(),
                stderr_excerpt: truncate_stderr(&enable.stderr),
            });
        }
        Ok(())
    }

    fn uninstall(&self) -> Result<(), AutostartError> {
        // disable --now（未登録でも無視）
        let _ = Self::run_systemctl(&["--user", "disable", "--now", "shikomi-daemon.service"]);

        // unit ファイル削除（NotFound → Ok、冪等）
        if let Some(unit) = Self::unit_path() {
            match std::fs::remove_file(&unit) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(AutostartError::IoError(e)),
            }
        }

        // daemon-reload（unit ファイル削除後の再読込）
        let _ = Self::run_systemctl(&["--user", "daemon-reload"]);
        Ok(())
    }

    fn is_registered(&self) -> bool {
        Self::unit_path().map(|p| p.exists()).unwrap_or(false)
    }

    fn install_hint(&self) -> Option<String> {
        Some("hint: to check status: systemctl --user status shikomi-daemon".to_owned())
    }
}

/// PATH 上にコマンドが存在するかを確認する（which crate の簡易代替）。
///
/// `std::env::split_paths` で OS 非依存に分割し、実行ファイルの存在を確認する。
fn is_on_path(cmd: &str) -> bool {
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            if dir.join(cmd).is_file() {
                return true;
            }
        }
    }
    false
}

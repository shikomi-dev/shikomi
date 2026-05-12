//! macOS launchd バックエンド（Sub-B Issue #127）。
//!
//! 設計根拠: docs/features/daemon-default-mode/autostart/detailed-design/launchd.md

use std::path::PathBuf;

use super::{resolve_daemon_path, truncate_stderr, AutostartBackend, AutostartError};

// plist テンプレート（{daemon_path} / {log_dir} を文字列置換する）
const PLIST_TEMPLATE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
    "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>dev.shikomi.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>{daemon_path}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
    <key>StandardOutPath</key>
    <string>{log_dir}/shikomi-daemon.log</string>
    <key>StandardErrorPath</key>
    <string>{log_dir}/shikomi-daemon.log</string>
</dict>
</plist>
"#;

const LABEL: &str = "dev.shikomi.daemon";

pub struct LaunchdBackend;

impl LaunchdBackend {
    fn plist_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join("Library/LaunchAgents/dev.shikomi.daemon.plist"))
    }

    fn log_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join("Library/Logs/shikomi"))
    }

    fn uid() -> u32 {
        // SAFETY: getuid は常に安全
        unsafe { libc::getuid() }
    }

    fn gui_service_target() -> String {
        format!("gui/{}/{LABEL}", Self::uid())
    }

    fn run_launchctl(args: &[&str]) -> Result<std::process::Output, AutostartError> {
        let out = std::process::Command::new("launchctl")
            .args(args)
            .output()?;
        Ok(out)
    }
}

impl AutostartBackend for LaunchdBackend {
    fn name(&self) -> &'static str {
        "LaunchdBackend"
    }

    fn install(&self) -> Result<(), AutostartError> {
        let daemon_path = resolve_daemon_path()?;
        let daemon_path_str = daemon_path.display().to_string();

        let log_dir = Self::log_dir().ok_or_else(|| {
            AutostartError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "cannot determine home directory",
            ))
        })?;
        std::fs::create_dir_all(&log_dir)?;

        let log_dir_str = log_dir.display().to_string();
        let plist_content = PLIST_TEMPLATE
            .replace("{daemon_path}", &daemon_path_str)
            .replace("{log_dir}", &log_dir_str);

        let launch_agents = dirs::home_dir()
            .ok_or_else(|| {
                AutostartError::IoError(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "cannot determine home directory",
                ))
            })?
            .join("Library/LaunchAgents");
        std::fs::create_dir_all(&launch_agents)?;

        let plist_path = Self::plist_path().ok_or_else(|| {
            AutostartError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "cannot determine home directory",
            ))
        })?;
        std::fs::write(&plist_path, plist_content)?;

        // 冪等確保: 事前に bootout（未登録エラーは無視）
        let _ = Self::run_launchctl(&["bootout", &Self::gui_service_target()]);

        // bootstrap で登録
        let plist_path_str = plist_path.display().to_string();
        let target = format!("gui/{}", Self::uid());
        let out = Self::run_launchctl(&["bootstrap", &target, &plist_path_str])?;
        if !out.status.success() {
            return Err(AutostartError::CommandFailed {
                cmd: format!("launchctl bootstrap {target} {plist_path_str}"),
                stderr_excerpt: truncate_stderr(&out.stderr),
            });
        }
        Ok(())
    }

    fn uninstall(&self) -> Result<(), AutostartError> {
        // bootout（未登録でも許容）
        let _ = Self::run_launchctl(&["bootout", &Self::gui_service_target()]);

        // plist 削除（NotFound → Ok、冪等）
        if let Some(plist) = Self::plist_path() {
            match std::fs::remove_file(&plist) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(AutostartError::IoError(e)),
            }
        }
        Ok(())
    }

    fn is_registered(&self) -> bool {
        Self::plist_path().map(|p| p.exists()).unwrap_or(false)
    }

    fn install_hint(&self) -> Option<String> {
        Some(format!(
            "hint: to start immediately: launchctl kickstart gui/{uid}/{LABEL}",
            uid = Self::uid(),
        ))
    }
}

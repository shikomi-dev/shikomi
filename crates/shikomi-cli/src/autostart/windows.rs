//! Windows Task Scheduler バックエンド（Sub-B Issue #127）。
//!
//! 設計根拠: docs/features/daemon-default-mode/autostart/detailed-design/windows.md

use super::{resolve_daemon_path, truncate_stderr, AutostartBackend, AutostartError};

const TASK_NAME: &str = r"shikomi\shikomi-daemon";

pub struct WindowsTaskSchedulerBackend;

impl WindowsTaskSchedulerBackend {
    fn run_schtasks(args: &[&str]) -> Result<std::process::Output, AutostartError> {
        let out = std::process::Command::new("schtasks").args(args).output()?;
        Ok(out)
    }
}

impl AutostartBackend for WindowsTaskSchedulerBackend {
    fn name(&self) -> &'static str {
        "WindowsTaskSchedulerBackend"
    }

    fn install(&self) -> Result<(), AutostartError> {
        let daemon_path = resolve_daemon_path()?;
        let daemon_path_str = daemon_path.display().to_string();

        // 冪等確保（事前確認）: 登録済みなら早期返却
        let query = Self::run_schtasks(&["/Query", "/TN", TASK_NAME])?;
        if query.status.success() {
            return Ok(());
        }

        // タスク登録（/F で既存タスクを強制上書き）
        let out = Self::run_schtasks(&[
            "/Create",
            "/SC",
            "ONLOGON",
            "/TN",
            TASK_NAME,
            "/TR",
            &daemon_path_str,
            "/F",
        ])?;
        if !out.status.success() {
            return Err(AutostartError::CommandFailed {
                cmd: format!(
                    "schtasks /Create /SC ONLOGON /TN {TASK_NAME} /TR {daemon_path_str} /F"
                ),
                stderr_excerpt: truncate_stderr(&out.stderr),
            });
        }
        Ok(())
    }

    fn uninstall(&self) -> Result<(), AutostartError> {
        let out = Self::run_schtasks(&["/Delete", "/TN", TASK_NAME, "/F"])?;
        if !out.status.success() {
            let stderr_str = String::from_utf8_lossy(&out.stderr);
            // 「未登録タスクの削除」は冪等 → Ok(())
            if stderr_str.contains("The system cannot find the file") {
                return Ok(());
            }
            return Err(AutostartError::CommandFailed {
                cmd: format!("schtasks /Delete /TN {TASK_NAME} /F"),
                stderr_excerpt: truncate_stderr(&out.stderr),
            });
        }
        Ok(())
    }

    fn is_registered(&self) -> bool {
        Self::run_schtasks(&["/Query", "/TN", TASK_NAME])
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    fn install_hint(&self) -> Option<String> {
        Some(format!(
            r#"hint: to start immediately: schtasks /Run /TN "{TASK_NAME}""#
        ))
    }
}

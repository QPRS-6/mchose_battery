//! "Start at login" support. Linux uses the XDG autostart directory, Windows
//! uses the per-user Run registry key.

#[cfg(target_os = "linux")]
mod imp {
    use std::fs;
    use std::path::PathBuf;

    use anyhow::{Context, Result};

    fn desktop_file() -> Result<PathBuf> {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .context("无法确定 XDG_CONFIG_HOME / HOME")?;
        Ok(config.join("autostart").join("mchose-battery.desktop"))
    }

    pub fn is_enabled() -> Result<bool> {
        Ok(desktop_file()?.exists())
    }

    pub fn set_enabled(enabled: bool) -> Result<()> {
        let path = desktop_file()?;
        if !enabled {
            match fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(err) => Err(err).with_context(|| format!("无法删除 {}", path.display())),
            }
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("无法创建 {}", parent.display()))?;
            }
            let exe = std::env::current_exe().context("无法确定程序路径")?;
            let exec = desktop_quote(&exe.display().to_string());
            let contents = format!(
                "[Desktop Entry]\n\
                 Type=Application\n\
                 Name=MCHOSE Battery\n\
                 Name[zh_CN]=迈从鼠标电量\n\
                 Comment=MCHOSE mouse battery in the system tray\n\
                 Comment[zh_CN]=在任务栏显示迈从鼠标电量\n\
                 Exec={exec}\n\
                 Icon=battery-good\n\
                 Terminal=false\n\
                 Categories=Utility;\n\
                 StartupNotify=false\n\
                 X-GNOME-Autostart-enabled=true\n"
            );
            fs::write(&path, contents).with_context(|| format!("无法写入 {}", path.display()))?;
            Ok(())
        }
    }

    fn desktop_quote(value: &str) -> String {
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    }
}

#[cfg(windows)]
mod imp {
    use anyhow::Result;
    use windows_registry::CURRENT_USER;

    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const VALUE_NAME: &str = "MCHOSE Battery";

    fn startup_command() -> Result<String> {
        Ok(format!("\"{}\"", std::env::current_exe()?.display()))
    }

    pub fn is_enabled() -> Result<bool> {
        let expected = startup_command()?;
        let Ok(key) = CURRENT_USER.open(RUN_KEY) else {
            return Ok(false);
        };
        Ok(key
            .get_string(VALUE_NAME)
            .map(|value| value == expected)
            .unwrap_or(false))
    }

    pub fn set_enabled(enabled: bool) -> Result<()> {
        if enabled {
            CURRENT_USER
                .create(RUN_KEY)?
                .set_string(VALUE_NAME, startup_command()?)?;
        } else if let Ok(key) = CURRENT_USER.options().read().write().open(RUN_KEY) {
            if key.get_value(VALUE_NAME).is_ok() {
                key.remove_value(VALUE_NAME)?;
            }
        }
        Ok(())
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use anyhow::Result;

    pub fn is_enabled() -> Result<bool> {
        Ok(false)
    }

    pub fn set_enabled(_enabled: bool) -> Result<()> {
        Ok(())
    }
}

pub use imp::{is_enabled, set_enabled};

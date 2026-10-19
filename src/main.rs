#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use anyhow::Result;

fn main() -> Result<()> {
    let args = mchose_battery::cli::parse(std::env::args().skip(1))?;

    // Release Windows builds use the GUI subsystem. Attach/create a console
    // before the first print so --help/--version/--scan/--json work from a
    // terminal as well as from Explorer.
    #[cfg(windows)]
    {
        if args.help || args.version || args.scan || args.json || args.once {
            windows_console::ensure();
        }
    }

    if args.help {
        mchose_battery::cli::print_help();
        return Ok(());
    }
    if args.version {
        println!("mchose-battery {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if args.scan || args.json || args.once {
        return mchose_battery::cli::run_cli(args);
    }

    #[cfg(target_os = "linux")]
    {
        mchose_battery::platform::linux::run(args)
    }

    #[cfg(windows)]
    {
        mchose_battery::platform::windows::run(args)
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    {
        anyhow::bail!("mchose-battery 目前支持 Windows 和 Linux/KDE");
    }
}

#[cfg(windows)]
mod windows_console {
    use std::ptr;

    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        AllocConsole, AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS,
        STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };

    /// A release build uses the `windows` subsystem so double-clicking does not
    /// open a console. For CLI modes, attach to the parent console or create
    /// one, and make sure Rust's stdout/stderr handles point at it.
    pub fn ensure() {
        unsafe {
            if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
                AllocConsole();
            }
            ensure_handle(STD_OUTPUT_HANDLE, "CONOUT$", FILE_GENERIC_WRITE);
            ensure_handle(STD_ERROR_HANDLE, "CONOUT$", FILE_GENERIC_WRITE);
            ensure_handle(STD_INPUT_HANDLE, "CONIN$", FILE_GENERIC_READ);
        }
    }

    unsafe fn ensure_handle(kind: u32, name: &str, access: u32) {
        let current = GetStdHandle(kind);
        if !current.is_null() && current != INVALID_HANDLE_VALUE {
            return;
        }

        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = CreateFileW(
            wide.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            OPEN_EXISTING,
            0,
            ptr::null_mut(),
        );
        if handle != INVALID_HANDLE_VALUE {
            SetStdHandle(kind, handle);
        }
    }
}

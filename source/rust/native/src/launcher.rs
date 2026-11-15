use anyhow::{Result, anyhow, bail};
use index_core::*;
use std::{
    env, fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

fn client() -> ureq::Agent {
    ureq::Agent::config_builder()
        .proxy(None)
        .timeout_global(Some(Duration::from_millis(1200)))
        .build()
        .into()
}
fn status() -> Option<Value> {
    let mut response = client().get("http://127.0.0.1:4173/api/app").call().ok()?;
    let text = response.body_mut().read_to_string().ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    (s(&value, "app") == "intelligence-index-local").then_some(value)
}
fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches(['\\', '/']))
}
fn application(path: &Path) -> bool {
    path.join("dist/index.html").is_file()
        && (path.join("public/models.json").is_file()
            || path.join("public/models.json.gz").is_file())
        && path.join("desktop/build-version.txt").is_file()
}
pub fn root() -> Result<PathBuf> {
    let binary = env::current_exe()?;
    let beside = binary.parent().unwrap();
    let documents = PathBuf::from(
        env::var_os("USERPROFILE").ok_or_else(|| anyhow!("The user folder is unavailable."))?,
    )
    .join("Documents/Intelligence Index");
    for candidate in [
        beside.to_path_buf(),
        beside.join("Intelligence Index"),
        documents,
    ] {
        if application(&candidate) && !candidate.join("Cargo.toml").is_file() {
            return Ok(candidate);
        }
    }
    bail!("Keep Intelligence Index.exe and all of its application folders together.")
}
pub fn activate(root: &Path, ensure_only: bool, stop_only: bool) -> Result<()> {
    let _guard = platform::lock()?;
    if stop_only {
        return stop(root, true);
    }
    let version = fs::read_to_string(root.join("desktop/build-version.txt"))?
        .trim()
        .to_string();
    if version.len() != 64 {
        bail!("The installed build identity is invalid.");
    }
    let ready = status().is_some_and(|v| {
        same_path(Path::new(s(&v, "project")), root)
            && s(&v, "build") == version
            && s(&v, "mode") == "production"
            && s(&v, "runtime") == "rust"
            && b(&v, "desktop")
            && b(&v, "ready")
    });
    if !ready {
        stop(root, false)?;
        let mut server = platform::spawn_server(root)?;
        let start = Instant::now();
        loop {
            if status().is_some_and(|v| {
                same_path(Path::new(s(&v, "project")), root)
                    && s(&v, "build") == version
                    && s(&v, "runtime") == "rust"
                    && b(&v, "ready")
            }) {
                break;
            }
            if server.has_exited()? {
                bail!(
                    "Intelligence Index could not open its local address. Keep the complete application folder together."
                );
            }
            if start.elapsed() > Duration::from_secs(25) {
                bail!("Intelligence Index is taking longer to open. Please open the app again.");
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
    if !ensure_only {
        platform::open(&format!("http://127.0.0.1:4173/?v={version}"))?;
    }
    Ok(())
}
pub fn stop(root: &Path, only_this: bool) -> Result<()> {
    let Some(pid) = platform::port_owner()? else {
        return Ok(());
    };
    let info = status();
    let owner = info
        .as_ref()
        .and_then(|v| v.get("project"))
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| root.to_path_buf());
    if only_this && !same_path(&owner, root) {
        return Ok(());
    }
    let process = platform::process_path(pid)?;
    let known = [
        owner.join("Intelligence Index.exe"),
        owner.join("desktop/IntelligenceIndex.Server.exe"),
        owner.join("desktop/runtime/node.exe"),
    ];
    if !known.iter().any(|expected| same_path(&process, expected)) {
        bail!("The current local listener belongs to another application.");
    }
    if let Some(info) = info {
        let key_file = owner.join(if s(&info, "runtime") == "rust" {
            "data/server.key"
        } else {
            "desktop/server-4173.key"
        });
        let key = fs::read_to_string(key_file)?;
        let response = client()
            .post("http://127.0.0.1:4173/api/desktop/restart")
            .header("x-intelligence-index-key", key.trim())
            .send_empty();
        if let Err(e) = response {
            if !platform::wait(pid, Duration::from_secs(5))? {
                return Err(e.into());
            }
        }
    } else {
        bail!("The local application is still starting. Open it again shortly.");
    }
    if !platform::wait(pid, Duration::from_secs(45))? {
        bail!(
            "The previous application is finishing its active data writes. Open Intelligence Index again shortly."
        );
    }
    Ok(())
}
#[cfg(windows)]
mod platform {
    use super::*;
    use std::ptr;
    use windows_sys::Win32::{
        Foundation::*,
        NetworkManagement::IpHelper::*,
        System::Threading::*,
        UI::{Shell::ShellExecuteW, WindowsAndMessaging::*},
    };
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    pub struct Guard(HANDLE);
    impl Drop for Guard {
        fn drop(&mut self) {
            unsafe {
                ReleaseMutex(self.0);
                CloseHandle(self.0);
            }
        }
    }
    pub fn lock() -> Result<Guard> {
        unsafe {
            let handle = CreateMutexW(
                ptr::null(),
                0,
                wide("Local\\ArtificialAnalysisIntelligenceIndexLaunch").as_ptr(),
            );
            if handle.is_null() {
                bail!("The application launch lock is unavailable.");
            }
            let result = WaitForSingleObject(handle, 30000);
            if result != WAIT_OBJECT_0 && result != WAIT_ABANDONED {
                CloseHandle(handle);
                bail!("Intelligence Index is still opening. Please open it again shortly.");
            }
            Ok(Guard(handle))
        }
    }
    pub struct ServerProcess(HANDLE);
    impl Drop for ServerProcess {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    impl ServerProcess {
        pub fn has_exited(&mut self) -> Result<bool> {
            unsafe {
                match WaitForSingleObject(self.0, 0) {
                    WAIT_OBJECT_0 => Ok(true),
                    WAIT_TIMEOUT => Ok(false),
                    _ => Err(std::io::Error::last_os_error().into()),
                }
            }
        }
    }
    pub fn spawn_server(root: &Path) -> Result<ServerProcess> {
        // The browser session owns the server lifetime. Do not inherit build
        // pipes or launcher handles, which otherwise keep capture readers open.
        let executable = root.join("Intelligence Index.exe");
        fn quote(value: &str) -> String {
            let mut out = String::from("\"");
            let mut slashes = 0;
            for c in value.chars() {
                if c == '\\' {
                    slashes += 1;
                    continue;
                }
                if c == '"' {
                    out.push_str(&"\\".repeat(slashes * 2 + 1));
                } else {
                    out.push_str(&"\\".repeat(slashes));
                }
                slashes = 0;
                out.push(c);
            }
            out.push_str(&"\\".repeat(slashes * 2));
            out.push('"');
            out
        }
        let application = wide(&executable.to_string_lossy());
        let directory = wide(&root.to_string_lossy());
        let mut command = wide(&format!(
            "{} --serve --desktop --root {}",
            quote(&executable.to_string_lossy()),
            quote(&root.to_string_lossy())
        ));
        unsafe {
            let mut startup: STARTUPINFOW = std::mem::zeroed();
            startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            startup.dwFlags = STARTF_USESHOWWINDOW;
            startup.wShowWindow = SW_HIDE as u16;
            let mut process: PROCESS_INFORMATION = std::mem::zeroed();
            if CreateProcessW(
                application.as_ptr(),
                command.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                0,
                CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP,
                ptr::null(),
                directory.as_ptr(),
                &startup,
                &mut process,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            CloseHandle(process.hThread);
            Ok(ServerProcess(process.hProcess))
        }
    }
    pub fn open(url: &str) -> Result<()> {
        unsafe {
            let result = ShellExecuteW(
                ptr::null_mut(),
                wide("open").as_ptr(),
                wide(url).as_ptr(),
                ptr::null(),
                ptr::null(),
                SW_SHOWNORMAL,
            );
            if result as usize <= 32 {
                bail!("The browser could not be opened.");
            }
        }
        Ok(())
    }
    pub fn port_owner() -> Result<Option<u32>> {
        unsafe {
            let mut size = 0;
            GetExtendedTcpTable(
                ptr::null_mut(),
                &mut size,
                0,
                2,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            );
            if size < 4 {
                return Ok(None);
            }
            let mut buffer = vec![0_u8; size as usize];
            let result = GetExtendedTcpTable(
                buffer.as_mut_ptr().cast(),
                &mut size,
                0,
                2,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            );
            if result != 0 {
                bail!("The local application port could not be inspected.");
            }
            let get =
                |offset: usize| u32::from_ne_bytes(buffer[offset..offset + 4].try_into().unwrap());
            for row in 0..get(0) as usize {
                let offset = 4 + row * 24;
                let raw = get(offset + 8);
                let port = ((raw & 255) << 8) | ((raw >> 8) & 255);
                let ip = get(offset + 4);
                if port == 4173 && (ip == 0 || ip == u32::from_ne_bytes([127, 0, 0, 1])) {
                    return Ok(Some(get(offset + 20)));
                }
            }
            Ok(None)
        }
    }
    pub fn process_path(pid: u32) -> Result<PathBuf> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                bail!("The current local server could not be identified.");
            }
            let mut buffer = vec![0_u16; 32768];
            let mut size = buffer.len() as u32;
            let success = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut size);
            CloseHandle(process);
            if success == 0 {
                bail!("The current local server path is unavailable.");
            }
            Ok(PathBuf::from(String::from_utf16_lossy(
                &buffer[..size as usize],
            )))
        }
    }
    pub fn wait(pid: u32, timeout: Duration) -> Result<bool> {
        unsafe {
            let process = OpenProcess(0x00100000, 0, pid);
            if process.is_null() {
                return Ok(true);
            }
            let result = WaitForSingleObject(process, timeout.as_millis() as u32);
            CloseHandle(process);
            Ok(result == WAIT_OBJECT_0)
        }
    }
}
#[cfg(not(windows))]
mod platform {
    use super::*;
    pub struct ServerProcess(std::process::Child);
    impl ServerProcess {
        pub fn has_exited(&mut self) -> Result<bool> {
            Ok(self.0.try_wait()?.is_some())
        }
    }
    pub fn spawn_server(root: &Path) -> Result<ServerProcess> {
        Ok(ServerProcess(
            std::process::Command::new(root.join("Intelligence Index.exe"))
                .args(["--serve", "--desktop", "--root"])
                .arg(root)
                .current_dir(root)
                .spawn()?,
        ))
    }
    pub fn lock() -> Result<()> {
        Ok(())
    }
    pub fn open(_: &str) -> Result<()> {
        Ok(())
    }
    pub fn port_owner() -> Result<Option<u32>> {
        Ok(None)
    }
    pub fn process_path(_: u32) -> Result<PathBuf> {
        bail!("Windows is required")
    }
    pub fn wait(_: u32, _: Duration) -> Result<bool> {
        Ok(true)
    }
}
pub fn show_error(message: &str) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let message: Vec<_> = message.encode_utf16().chain(Some(0)).collect();
        let title: Vec<_> = "Intelligence Index".encode_utf16().chain(Some(0)).collect();
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

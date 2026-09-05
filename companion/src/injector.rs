use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionMethod {
    Standard,
    Apc,
    Nt,
    ManualMap,
}

impl InjectionMethod {
    pub fn as_arg(&self) -> &'static str {
        match self {
            InjectionMethod::Standard => "standard",
            InjectionMethod::Apc => "apc",
            InjectionMethod::Nt => "nt",
            InjectionMethod::ManualMap => "manual_map",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InjectRequest {
    pub app_id: u32,
    pub dll_path: PathBuf,
    pub injector_exe: PathBuf,
    pub method: InjectionMethod,
}

impl InjectRequest {
    pub fn new(app_id: u32, dll_path: PathBuf, injector_exe: PathBuf) -> Self {
        Self {
            app_id,
            dll_path,
            injector_exe,
            method: InjectionMethod::Apc,
        }
    }

    pub fn dll_exists(&self) -> bool {
        self.dll_path.is_file()
    }

    pub fn target_label(&self) -> String {
        self.dll_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("deadass-dll.dll")
            .to_string()
    }

    #[cfg(unix)]
    pub fn dll_wine_path(&self) -> String {
        crate::discovery::to_wine_path(&self.dll_path)
    }

    #[cfg(unix)]
    pub fn launch_command(&self) -> (String, Vec<String>) {
        (
            "protontricks-launch".to_string(),
            vec![
                "--no-bwrap".to_string(),
                "--appid".to_string(),
                self.app_id.to_string(),
                self.injector_exe.to_string_lossy().to_string(),
                self.dll_wine_path(),
                "--method".to_string(),
                self.method.as_arg().to_string(),
            ],
        )
    }

    #[cfg(windows)]
    pub fn launch_command(&self) -> (String, Vec<String>) {
        (
            self.injector_exe.to_string_lossy().to_string(),
            vec![
                self.dll_path.to_string_lossy().to_string(),
                "--method".to_string(),
                self.method.as_arg().to_string(),
            ],
        )
    }
}

pub fn dll_default_path(home: &Path) -> PathBuf {
    home.join("deadass").join("deadass-dll.dll")
}

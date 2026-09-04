use crate::discovery::to_wine_path;
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
    pub dll_linux_path: PathBuf,
    pub injector_exe: PathBuf,
    pub method: InjectionMethod,
}

impl InjectRequest {
    pub fn new(app_id: u32, dll_linux_path: PathBuf, injector_exe: PathBuf) -> Self {
        Self {
            app_id,
            dll_linux_path,
            injector_exe,
            method: InjectionMethod::Apc,
        }
    }

    pub fn dll_wine_path(&self) -> String {
        to_wine_path(&self.dll_linux_path)
    }

    pub fn protontricks_args(&self) -> Vec<String> {
        vec![
            "--no-bwrap".to_string(),
            "--appid".to_string(),
            self.app_id.to_string(),
            self.injector_exe.to_string_lossy().to_string(),
            self.dll_wine_path(),
            "--method".to_string(),
            self.method.as_arg().to_string(),
        ]
    }

    pub fn dll_exists(&self) -> bool {
        self.dll_linux_path.is_file()
    }

    pub fn target_label(&self) -> String {
        self.dll_linux_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("deadass-dll.dll")
            .to_string()
    }
}

pub fn dll_default_path(home: &Path) -> PathBuf {
    home.join("deadass").join("deadass-dll.dll")
}

use serde::Serialize;
use sysinfo::{Disks, System};

#[derive(Serialize, Clone, Debug)]
pub struct Hardware {
    pub os: String, pub arch: String, pub cpu_brand: String, pub physical_cores: usize, pub logical_cores: usize,
    pub total_ram_mib: u64, pub available_ram_mib: u64,
    /// x86 only: AVX2 / AVX-512 matter a lot for llama.cpp CPU speed.
    pub avx2: bool, pub avx512: bool,
    /// Apple Silicon has unified memory and Metal; elsewhere the GPU is not probed (llama.cpp GPU offload is NOT forced).
    pub apple_silicon: bool,
}

pub fn detect() -> Hardware {
    let mut s = System::new(); s.refresh_memory(); s.refresh_cpu_all();
    let (avx2, avx512) = x86_features();
    Hardware {
        os: System::long_os_version().or_else(System::name).unwrap_or_else(|| std::env::consts::OS.into()), arch: std::env::consts::ARCH.into(),
        cpu_brand: s.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default(),
        physical_cores: System::physical_core_count().unwrap_or_else(|| s.cpus().len().max(1)), logical_cores: s.cpus().len().max(1),
        total_ram_mib: s.total_memory() / 1_048_576, available_ram_mib: s.available_memory() / 1_048_576,
        avx2, avx512, apple_silicon: cfg!(all(target_os = "macos", target_arch = "aarch64")),
    }
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn x86_features() -> (bool, bool) { (std::is_x86_feature_detected!("avx2"), std::is_x86_feature_detected!("avx512f")) }
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn x86_features() -> (bool, bool) { (false, false) }

/// Free space (bytes) on the disk holding `path`: longest matching mount point.
pub fn free_disk_bytes(path: &std::path::Path) -> Option<u64> {
    let p = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Disks::new_with_refreshed_list().list().iter().filter(|d| p.starts_with(d.mount_point())).max_by_key(|d| d.mount_point().as_os_str().len()).map(|d| d.available_space())
}

/// Threads for generation: physical cores (hyper-threads rarely help memory-bound decoding), at least 1.
pub fn auto_threads(h: &Hardware) -> usize { h.physical_cores.max(1) }

/// Cross-platform hostname retrieval.
pub fn gethostname() -> String {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        let result = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if result != 0 {
            return "unknown".to_string();
        }
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..len]).to_string()
    }
    #[cfg(windows)]
    {
        // Use GetComputerNameExW for reliable FQDN retrieval on Windows.
        // Falls back to env vars if the API call fails.
        win_hostname().unwrap_or_else(|| {
            std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .unwrap_or_else(|_| "unknown".to_string())
        })
    }
}

/// Windows: retrieve hostname via Win32 API for reliability.
#[cfg(windows)]
fn win_hostname() -> Option<String> {
    // Use std::process::Command to call hostname.exe — works everywhere, no extra deps
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|out| {
            let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if name.is_empty() {
                None
            } else {
                Some(name)
            }
        })
}

/// Detect all non-loopback IP addresses on this machine.
/// Returns both IPv4 and IPv6 addresses as strings.
/// Useful for Azure/cloud VMs where FQDN may not be meaningful.
pub fn get_ip_addresses() -> Vec<String> {
    let mut addresses = Vec::new();

    // Use sysinfo network interfaces
    let networks = sysinfo::Networks::new_with_refreshed_list();
    for (_name, data) in &networks {
        for ip in data.ip_networks() {
            let addr = ip.addr;
            // Skip loopback
            if addr.is_loopback() {
                continue;
            }
            addresses.push(addr.to_string());
        }
    }

    addresses.sort();
    addresses.dedup();
    addresses
}

/// System information collected once at startup.
#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub os_name: String,
    pub os_version: String,
    pub cpu_arch: String,
    pub cpu_cores: u32,
    pub total_memory_mb: u64,
    pub disk_total_gb: u64,
}

/// Collect static system information (OS, CPU, memory, disk).
pub fn get_system_info() -> SystemInfo {
    use sysinfo::{Disks, System};

    let mut sys = System::new_all();
    sys.refresh_all();

    // OS info
    let os_name = System::name().unwrap_or_else(|| "Unknown".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "Unknown".to_string());

    // CPU info
    let cpu_arch = std::env::consts::ARCH.to_string();
    let cpu_cores = sys.cpus().len() as u32;

    // Memory info (convert from bytes to MB)
    let total_memory_mb = sys.total_memory() / (1024 * 1024);

    // Disk info - sum all disks or take the largest one
    let disks = Disks::new_with_refreshed_list();
    let disk_total_gb =
        disks.iter().map(|d| d.total_space()).max().unwrap_or(0) / (1024 * 1024 * 1024);

    SystemInfo {
        os_name,
        os_version,
        cpu_arch,
        cpu_cores,
        total_memory_mb,
        disk_total_gb,
    }
}

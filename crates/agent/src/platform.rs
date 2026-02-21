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
    #[cfg(not(unix))]
    {
        std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "unknown".to_string())
    }
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

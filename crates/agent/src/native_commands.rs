use serde_json::{json, Value};
use sysinfo::System;

/// Built-in native commands that don't require external scripts.

pub fn disk_space(path: &str) -> Value {
    let disks = sysinfo::Disks::new_with_refreshed_list();

    for disk in disks.list() {
        let mount = disk.mount_point().to_string_lossy().to_string();
        if mount == path || path == "/" {
            return json!({
                "mount_point": mount,
                "total_bytes": disk.total_space(),
                "available_bytes": disk.available_space(),
                "used_bytes": disk.total_space() - disk.available_space(),
                "usage_pct": ((disk.total_space() - disk.available_space()) as f64 / disk.total_space() as f64) * 100.0,
            });
        }
    }

    json!({"error": "path not found"})
}

pub fn memory() -> Value {
    let mut sys = System::new();
    sys.refresh_memory();

    json!({
        "total_bytes": sys.total_memory(),
        "used_bytes": sys.used_memory(),
        "available_bytes": sys.available_memory(),
        "swap_total_bytes": sys.total_swap(),
        "swap_used_bytes": sys.used_swap(),
        "usage_pct": (sys.used_memory() as f64 / sys.total_memory() as f64) * 100.0,
    })
}

pub fn cpu() -> Value {
    let mut sys = System::new();
    sys.refresh_cpu_usage();
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_usage();

    let cpus: Vec<Value> = sys.cpus().iter().enumerate().map(|(i, cpu)| {
        json!({
            "id": i,
            "usage_pct": cpu.cpu_usage(),
            "name": cpu.name(),
        })
    }).collect();

    let load = System::load_average();
    json!({
        "global_usage_pct": load.one,
        "cpu_count": sys.cpus().len(),
        "cpus": cpus,
    })
}

pub fn process_check(name: &str) -> Value {
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let matching: Vec<Value> = sys.processes().iter()
        .filter(|(_, p)| {
            let proc_name = p.name().to_string_lossy();
            proc_name.contains(name)
        })
        .map(|(pid, p)| {
            json!({
                "pid": pid.as_u32(),
                "name": p.name().to_string_lossy(),
                "cpu_usage": p.cpu_usage(),
                "memory_bytes": p.memory(),
                "status": format!("{:?}", p.status()),
            })
        })
        .collect();

    json!({
        "process_name": name,
        "found": !matching.is_empty(),
        "count": matching.len(),
        "processes": matching,
    })
}

pub fn tcp_port(port: u16) -> Value {
    use std::net::TcpStream;
    let addr = format!("127.0.0.1:{}", port);
    let is_open = TcpStream::connect_timeout(
        &addr.parse().unwrap(),
        std::time::Duration::from_secs(2),
    )
    .is_ok();

    json!({
        "port": port,
        "is_open": is_open,
        "address": addr,
    })
}

pub fn http_check(url: &str) -> Value {
    json!({
        "url": url,
        "status": "not_implemented_in_sync_mode",
    })
}

pub fn load_average() -> Value {
    let loadavg = System::load_average();
    json!({
        "one": loadavg.one,
        "five": loadavg.five,
        "fifteen": loadavg.fifteen,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_returns_valid_json() {
        let result = memory();
        assert!(result.get("total_bytes").is_some());
        assert!(result.get("used_bytes").is_some());
        assert!(result.get("usage_pct").is_some());
    }

    #[test]
    fn test_cpu_returns_valid_json() {
        let result = cpu();
        assert!(result.get("global_usage_pct").is_some());
        assert!(result.get("cpu_count").is_some());
    }

    #[test]
    fn test_load_average_returns_valid_json() {
        let result = load_average();
        assert!(result.get("one").is_some());
        assert!(result.get("five").is_some());
        assert!(result.get("fifteen").is_some());
    }

    #[test]
    fn test_tcp_port_closed() {
        let result = tcp_port(59999);
        assert_eq!(result["is_open"], false);
    }

    #[test]
    fn test_process_check() {
        let result = process_check("nonexistent_process_xyz");
        assert_eq!(result["found"], false);
        assert_eq!(result["count"], 0);
    }
}

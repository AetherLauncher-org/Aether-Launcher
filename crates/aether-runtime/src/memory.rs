//! Installed and available physical memory, used to size the game's heap.
//!
//! This is read-only and takes no input: the numbers come from the operating
//! system (the Windows API, `/proc/meminfo`, `sysctl` and `vm_stat`), never
//! from a caller-supplied path.

use serde::{Deserialize, Serialize};

const MIB: u64 = 1024 * 1024;

/// Physical memory in MiB. `available_mib` is `None` when the platform did not
/// report it; callers must not invent a value in that case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemMemory {
    pub total_mib: u64,
    pub available_mib: Option<u64>,
}

/// Read the machine's memory, or `None` when it cannot be determined.
#[must_use]
pub fn system_memory() -> Option<SystemMemory> {
    let memory = read_platform_memory()?;
    // A report that contradicts itself is worse than none.
    if memory.total_mib == 0
        || memory
            .available_mib
            .is_some_and(|free| free > memory.total_mib)
    {
        return None;
    }
    Some(memory)
}

#[cfg(windows)]
fn read_platform_memory() -> Option<SystemMemory> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    // SAFETY: `MEMORYSTATUSEX` is plain data; zeroed is a valid starting value
    // and `dwLength` is set before the call as the API requires. The pointer
    // is to a live local for the duration of the call.
    let status = unsafe {
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = u32::try_from(std::mem::size_of::<MEMORYSTATUSEX>()).ok()?;
        if GlobalMemoryStatusEx(&mut status) == 0 {
            return None;
        }
        status
    };
    Some(SystemMemory {
        total_mib: status.ullTotalPhys / MIB,
        available_mib: Some(status.ullAvailPhys / MIB),
    })
}

#[cfg(target_os = "linux")]
fn read_platform_memory() -> Option<SystemMemory> {
    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

#[cfg(target_os = "macos")]
fn read_platform_memory() -> Option<SystemMemory> {
    use std::process::Command;

    let output = |program: &str, args: &[&str]| {
        Command::new(program)
            .args(args)
            .output()
            .ok()
            .filter(|result| result.status.success())
            .map(|result| String::from_utf8_lossy(&result.stdout).into_owned())
    };
    let total_bytes: u64 = output("sysctl", &["-n", "hw.memsize"])?
        .trim()
        .parse()
        .ok()?;
    let available_mib = output("vm_stat", &[]).and_then(|text| parse_vm_stat(&text));
    Some(SystemMemory {
        total_mib: total_bytes / MIB,
        available_mib,
    })
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn read_platform_memory() -> Option<SystemMemory> {
    None
}

/// Parse `/proc/meminfo`. Values are in kB; `MemAvailable` is preferred and
/// `MemFree` is not a substitute (it ignores reclaimable cache).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_meminfo(text: &str) -> Option<SystemMemory> {
    let field = |name: &str| -> Option<u64> {
        text.lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix(':'))
            .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
    };
    let total_kib = field("MemTotal")?;
    Some(SystemMemory {
        total_mib: total_kib / 1024,
        available_mib: field("MemAvailable").map(|kib| kib / 1024),
    })
}

/// Parse `vm_stat`: free plus inactive plus speculative pages are memory the
/// system can hand out without swapping.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_vm_stat(text: &str) -> Option<u64> {
    let page_size: u64 = text
        .lines()
        .next()?
        .split("page size of")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let pages = |name: &str| -> u64 {
        text.lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix(':'))
            .and_then(|rest| rest.trim().trim_end_matches('.').parse().ok())
            .unwrap_or(0)
    };
    let free = pages("Pages free") + pages("Pages inactive") + pages("Pages speculative");
    (free > 0).then_some(free * page_size / MIB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux_meminfo_and_prefers_mem_available() {
        let text = "MemTotal:       16384000 kB\nMemFree:          500000 kB\nMemAvailable:    8192000 kB\nBuffers: 1 kB\n";
        assert_eq!(
            parse_meminfo(text),
            Some(SystemMemory {
                total_mib: 16000,
                available_mib: Some(8000)
            })
        );
    }

    #[test]
    fn meminfo_without_mem_available_reports_none_instead_of_guessing() {
        let text = "MemTotal: 2048000 kB\nMemFree: 1000000 kB\n";
        assert_eq!(
            parse_meminfo(text),
            Some(SystemMemory {
                total_mib: 2000,
                available_mib: None
            })
        );
    }

    #[test]
    fn rejects_meminfo_without_a_total() {
        assert_eq!(parse_meminfo("MemFree: 1 kB\n"), None);
        assert_eq!(parse_meminfo(""), None);
        assert_eq!(parse_meminfo("MemTotal: lots kB\n"), None);
    }

    #[test]
    fn sums_free_inactive_and_speculative_pages_from_vm_stat() {
        let text = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\n\
                    Pages free:                               1000.\n\
                    Pages active:                             9999.\n\
                    Pages inactive:                           2000.\n\
                    Pages speculative:                         500.\n";
        // (1000 + 2000 + 500) pages * 16 KiB = 56000 KiB = 54 MiB (rounded down)
        assert_eq!(parse_vm_stat(text), Some(3500 * 16384 / MIB));
    }

    #[test]
    fn vm_stat_without_a_page_size_or_pages_is_unknown() {
        assert_eq!(parse_vm_stat("garbage"), None);
        assert_eq!(
            parse_vm_stat("(page size of 4096 bytes)\nPages free: 0.\n"),
            None
        );
    }

    #[test]
    fn serialises_with_camel_case_keys() {
        let value = serde_json::to_value(SystemMemory {
            total_mib: 16000,
            available_mib: None,
        })
        .unwrap();
        assert_eq!(value["totalMib"], 16000);
        assert!(value["availableMib"].is_null());
        assert!(value.get("total_mib").is_none());
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    #[test]
    fn the_real_machine_reports_consistent_numbers() {
        let memory = system_memory().expect("this platform should report memory");
        assert!(
            memory.total_mib >= 256,
            "implausibly small total: {memory:?}"
        );
        if let Some(free) = memory.available_mib {
            assert!(free <= memory.total_mib);
        }
    }
}

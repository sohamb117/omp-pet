//! On-demand counters for local benchmarking; no sampling or reporting in the background.
use serde::Serialize;

#[derive(Serialize)]
pub struct ProcessUsage {
    pub cpu_seconds: f64,
    pub resident_bytes: u64,
    pub peak_resident_bytes: u64,
}

// libc recommends a separate Mach crate; retain its existing ABI binding for this one query.
#[allow(deprecated)]
pub fn process_usage() -> Option<ProcessUsage> {
    // SAFETY: both APIs write to correctly sized, initialized structures for this process.
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        let mut memory: libc::mach_task_basic_info = std::mem::zeroed();
        let mut count = libc::MACH_TASK_BASIC_INFO_COUNT;
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) != 0
            || libc::task_info(
                libc::mach_task_self(),
                libc::MACH_TASK_BASIC_INFO,
                (&mut memory as *mut libc::mach_task_basic_info).cast(),
                &mut count,
            ) != libc::KERN_SUCCESS
        {
            return None;
        }
        Some(ProcessUsage {
            cpu_seconds: usage.ru_utime.tv_sec as f64
                + usage.ru_stime.tv_sec as f64
                + (usage.ru_utime.tv_usec + usage.ru_stime.tv_usec) as f64 / 1_000_000.,
            resident_bytes: memory.resident_size,
            // Darwin reports ru_maxrss in bytes, unlike Linux.
            peak_resident_bytes: usage.ru_maxrss as u64,
        })
    }
}

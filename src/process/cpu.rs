use std::{
    collections::HashMap,
    fs, io,
    path::Path,
    time::{Duration, Instant},
};

use super::{ProcessIdentity, ProcessSnapshot};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SystemMetrics {
    pub cpu_percent: Option<f32>,
    pub memory_used_percent: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CpuSample {
    total_ticks: u64,
    idle_ticks: u64,
    logical_cpus: u32,
}

#[derive(Debug, Clone, Copy)]
struct ProcessCounters {
    cpu_ticks: u64,
    read_bytes: Option<u64>,
    write_bytes: Option<u64>,
}

#[derive(Debug, Default)]
pub struct DeltaTracker {
    cpu: Option<CpuSample>,
    processes: HashMap<ProcessIdentity, ProcessCounters>,
    captured_at: Option<Instant>,
}

impl DeltaTracker {
    pub fn update(&mut self, root: &Path, processes: &mut [ProcessSnapshot]) -> SystemMetrics {
        let current_cpu = read_cpu_sample(root).ok();
        let memory_used_percent = read_memory_used_percent(root).ok().flatten();
        let now = Instant::now();
        let elapsed = self.captured_at.map_or(Duration::ZERO, |previous| {
            now.saturating_duration_since(previous)
        });

        let total_delta = current_cpu.zip(self.cpu).map_or(0, |(current, previous)| {
            current.total_ticks.saturating_sub(previous.total_ticks)
        });
        let logical_cpus = current_cpu.map_or(1, |sample| sample.logical_cpus.max(1));
        for process in &mut *processes {
            let previous = self.processes.get(&process.identity).copied();
            process.cpu_percent = previous.map_or(0.0, |previous| {
                process.cpu_time_ticks.saturating_sub(previous.cpu_ticks) as f64
                    / total_delta.max(1) as f64
                    * f64::from(logical_cpus)
                    * 100.0
            }) as f32;
            process.read_rate_bytes = byte_rate(
                previous.and_then(|counters| counters.read_bytes),
                process.read_bytes,
                elapsed,
            );
            process.write_rate_bytes = byte_rate(
                previous.and_then(|counters| counters.write_bytes),
                process.write_bytes,
                elapsed,
            );
        }

        let cpu_percent = current_cpu.zip(self.cpu).and_then(|(current, previous)| {
            system_cpu_percent(previous.total_ticks, previous.idle_ticks, current)
        });
        self.processes = processes
            .iter()
            .map(|process| {
                (
                    process.identity,
                    ProcessCounters {
                        cpu_ticks: process.cpu_time_ticks,
                        read_bytes: process.read_bytes,
                        write_bytes: process.write_bytes,
                    },
                )
            })
            .collect();
        self.cpu = current_cpu;
        self.captured_at = Some(now);

        SystemMetrics {
            cpu_percent,
            memory_used_percent,
        }
    }
}

fn byte_rate(previous: Option<u64>, current: Option<u64>, elapsed: Duration) -> Option<f64> {
    let seconds = elapsed.as_secs_f64();
    if seconds <= f64::EPSILON {
        return None;
    }
    Some(current?.saturating_sub(previous?) as f64 / seconds)
}

fn read_cpu_sample(root: &Path) -> io::Result<CpuSample> {
    parse_cpu_stat(&fs::read(root.join("stat"))?).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "missing aggregate CPU statistics",
        )
    })
}

fn parse_cpu_stat(contents: &[u8]) -> Option<CpuSample> {
    let text = std::str::from_utf8(contents).ok()?;
    let aggregate = text.lines().find(|line| line.starts_with("cpu "))?;
    let ticks: Vec<u64> = aggregate
        .split_ascii_whitespace()
        .skip(1)
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if ticks.len() < 4 {
        return None;
    }
    let total_ticks = ticks.iter().copied().fold(0_u64, u64::saturating_add);
    let idle_ticks = ticks[3].saturating_add(ticks.get(4).copied().unwrap_or(0));
    let logical_cpus = u32::try_from(
        text.lines()
            .filter(|line| {
                line.strip_prefix("cpu").is_some_and(|suffix| {
                    suffix
                        .chars()
                        .next()
                        .is_some_and(|character| character.is_ascii_digit())
                        && suffix
                            .split_ascii_whitespace()
                            .next()
                            .is_some_and(|id| id.chars().all(|c| c.is_ascii_digit()))
                })
            })
            .count(),
    )
    .unwrap_or(u32::MAX)
    .max(1);
    Some(CpuSample {
        total_ticks,
        idle_ticks,
        logical_cpus,
    })
}

fn system_cpu_percent(previous_total: u64, previous_idle: u64, current: CpuSample) -> Option<f32> {
    let total_delta = current.total_ticks.saturating_sub(previous_total);
    if total_delta == 0 {
        return None;
    }
    let idle_delta = current.idle_ticks.saturating_sub(previous_idle);
    Some((total_delta.saturating_sub(idle_delta) as f64 / total_delta as f64 * 100.0) as f32)
}

fn read_memory_used_percent(root: &Path) -> io::Result<Option<f32>> {
    Ok(parse_memory_used_percent(&fs::read(root.join("meminfo"))?))
}

fn parse_memory_used_percent(contents: &[u8]) -> Option<f32> {
    let text = std::str::from_utf8(contents).ok()?;
    let mut total = None;
    let mut available = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("MemTotal:") {
            total = value.split_ascii_whitespace().next()?.parse::<u64>().ok();
        } else if let Some(value) = line.strip_prefix("MemAvailable:") {
            available = value.split_ascii_whitespace().next()?.parse::<u64>().ok();
        }
    }
    let total = total?;
    if total == 0 {
        return None;
    }
    Some((total.saturating_sub(available?) as f64 / total as f64 * 100.0) as f32)
}

#[cfg(test)]
mod tests {
    use super::{parse_cpu_stat, parse_memory_used_percent, system_cpu_percent};

    #[test]
    fn parses_cpu_and_memory_samples() {
        let sample = parse_cpu_stat(b"cpu  10 2 3 80 5 0 0 0\ncpu0 1 0 0 9\ncpu1 1 0 0 9\n")
            .expect("CPU sample");
        assert_eq!(sample.total_ticks, 100);
        assert_eq!(sample.idle_ticks, 85);
        assert_eq!(sample.logical_cpus, 2);
        assert_eq!(
            parse_memory_used_percent(b"MemTotal: 1000 kB\nMemAvailable: 250 kB\n"),
            Some(75.0)
        );
    }

    #[test]
    fn handles_zero_cpu_delta() {
        let current = parse_cpu_stat(b"cpu  10 2 3 80 5\ncpu0 1 0 0 9\n").expect("sample");
        assert_eq!(system_cpu_percent(100, 85, current), None);
    }

    #[test]
    fn calculates_busy_cpu_delta() {
        let current = parse_cpu_stat(b"cpu  20 2 3 90 5\ncpu0 1 0 0 9\n").expect("sample");
        assert_eq!(system_cpu_percent(100, 85, current), Some(50.0));
    }
}

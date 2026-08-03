use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};

use super::{ApplicationResources, ProcessIdentity};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResourceTrend {
    pub duration: Duration,
    pub memory_delta_bytes: i128,
    pub average_cpu_percent: f32,
}

#[derive(Debug, Clone, Copy)]
struct Sample {
    captured_at: Instant,
    memory_bytes: u64,
    cpu_percent: f32,
    complete_pss: bool,
}

#[derive(Debug)]
pub struct TrendTracker {
    window: Duration,
    samples: HashMap<ProcessIdentity, VecDeque<Sample>>,
}

impl TrendTracker {
    #[must_use]
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            samples: HashMap::new(),
        }
    }

    pub fn update(&mut self, resources: &HashMap<ProcessIdentity, ApplicationResources>) {
        self.update_at(Instant::now(), resources);
    }

    fn update_at(
        &mut self,
        now: Instant,
        resources: &HashMap<ProcessIdentity, ApplicationResources>,
    ) {
        let active: HashSet<_> = resources.keys().copied().collect();
        self.samples.retain(|identity, _| active.contains(identity));
        for (identity, resource) in resources {
            let samples = self.samples.entry(*identity).or_default();
            samples.push_back(Sample {
                captured_at: now,
                memory_bytes: resource.preferred_memory_bytes(),
                cpu_percent: resource.cpu_percent,
                complete_pss: resource.has_complete_pss(),
            });
            while samples.front().is_some_and(|sample| {
                now.saturating_duration_since(sample.captured_at) > self.window
            }) {
                samples.pop_front();
            }
        }
    }

    #[must_use]
    pub fn summary(&self, identity: ProcessIdentity) -> Option<ResourceTrend> {
        let samples = self.samples.get(&identity)?;
        let last = samples.back()?;
        let comparable = samples
            .iter()
            .filter(|sample| sample.complete_pss == last.complete_pss)
            .collect::<Vec<_>>();
        let first = comparable.first()?;
        if comparable.len() < 2 || last.captured_at <= first.captured_at {
            return None;
        }
        let cpu_sum: f32 = comparable.iter().map(|sample| sample.cpu_percent).sum();
        Some(ResourceTrend {
            duration: last
                .captured_at
                .saturating_duration_since(first.captured_at),
            memory_delta_bytes: i128::from(last.memory_bytes) - i128::from(first.memory_bytes),
            average_cpu_percent: cpu_sum / comparable.len() as f32,
        })
    }
}

impl Default for TrendTracker {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        time::{Duration, Instant},
    };

    use super::TrendTracker;
    use crate::process::{ApplicationResources, ProcessIdentity};

    #[test]
    fn reports_a_bounded_memory_delta_and_cpu_average() {
        let identity = ProcessIdentity {
            pid: 10,
            start_time_ticks: 20,
        };
        let start = Instant::now();
        let mut tracker = TrendTracker::new(Duration::from_secs(30));
        let mut resources = HashMap::from([(
            identity,
            ApplicationResources {
                rss_bytes: 100,
                cpu_percent: 10.0,
                process_count: 1,
                ..Default::default()
            },
        )]);
        tracker.update_at(start, &resources);
        resources.get_mut(&identity).expect("resource").rss_bytes = 160;
        resources.get_mut(&identity).expect("resource").cpu_percent = 30.0;
        tracker.update_at(start + Duration::from_secs(10), &resources);

        let trend = tracker.summary(identity).expect("two samples");
        assert_eq!(trend.memory_delta_bytes, 60);
        assert_eq!(trend.duration, Duration::from_secs(10));
        assert_eq!(trend.average_cpu_percent, 20.0);
    }

    #[test]
    fn prunes_vanished_identities_and_requires_two_samples() {
        let identity = ProcessIdentity {
            pid: 1,
            start_time_ticks: 1,
        };
        let now = Instant::now();
        let mut tracker = TrendTracker::default();
        tracker.update_at(
            now,
            &HashMap::from([(
                identity,
                ApplicationResources {
                    rss_bytes: 1,
                    process_count: 1,
                    ..Default::default()
                },
            )]),
        );
        assert!(tracker.summary(identity).is_none());
        tracker.update_at(now + Duration::from_secs(1), &HashMap::new());
        assert!(tracker.summary(identity).is_none());
    }
}

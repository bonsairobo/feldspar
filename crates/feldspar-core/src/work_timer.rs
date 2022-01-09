use std::convert::TryInto;
use std::time::Duration;

pub struct WorkTimer {
    total_cpu_time: Duration,
    items_completed: u32,
}

impl WorkTimer {
    pub fn start() -> Self {
        Self {
            total_cpu_time: Duration::new(0, 0),
            items_completed: 0,
        }
    }

    pub fn total_cpu_time(&self) -> Duration {
        self.total_cpu_time
    }

    pub fn items_completed(&self) -> u32 {
        self.items_completed
    }

    pub fn complete_item(&mut self, d: Duration) {
        self.total_cpu_time += d;
        self.items_completed += 1;
    }

    pub fn average_cpu_time_us(&self) -> u32 {
        let frame_cpu_time_us: u32 = self
            .total_cpu_time
            .as_micros()
            .try_into()
            .unwrap_or(u32::MAX);

        frame_cpu_time_us / self.items_completed.max(1)
    }
}

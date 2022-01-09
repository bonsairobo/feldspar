use crate::work_timer::WorkTimer;

use std::time::Duration;

pub struct FrameBudget {
    num_threads: u32,
    item_time_estimate_us: u32,
    target_frame_time_us: u32,
    timer: Option<WorkTimer>,
}

impl FrameBudget {
    pub fn new(
        num_threads: u32,
        target_frame_time_us: u32,
        initial_item_time_estimate_us: u32,
    ) -> Self {
        Self {
            num_threads,
            target_frame_time_us,
            item_time_estimate_us: initial_item_time_estimate_us,
            timer: None,
        }
    }

    pub fn reset_timer(&mut self) {
        self.timer = Some(WorkTimer::start());
    }

    pub fn complete_item(&mut self, cpu_time: Duration) {
        let timer = self.timer.as_mut().expect("Reset the timer first");
        timer.complete_item(cpu_time);
    }

    pub fn update_estimate(&mut self) {
        if let Some(timer) = self.timer.as_ref() {
            if timer.items_completed() > 0 {
                self.item_time_estimate_us = timer.average_cpu_time_us();
            }
        }
    }

    pub fn items_per_frame(&self) -> u32 {
        (self.target_frame_time_us * self.num_threads) / self.item_time_estimate_us.max(1)
    }
}

/* Copyright 2023 shadow3aaa@gitbub.com
*
*  Licensed under the Apache License, Version 2.0 (the "License");
*  you may not use this file except in compliance with the License.
*  You may obtain a copy of the License at
*
*      http://www.apache.org/licenses/LICENSE-2.0
*
*  Unless required by applicable law or agreed to in writing, software
*  distributed under the License is distributed on an "AS IS" BASIS,
*  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
*  See the License for the specific language governing permissions and
*  limitations under the License. */
pub mod config;
mod utils;

use std::{
    cmp::{self, Ordering as CmpOrdering},
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use atomic::{Atomic, Ordering};
use cpu_cycles_reader::Cycles;
use fas_rs_fw::{prelude::*, write_pool::WritePool};

use log::{debug, info, trace};

use touch_event::TouchListener;
use yata::{methods::SMA, prelude::*};

const BURST_DEFAULT: usize = 0;
const BURST_MAX: usize = 2;
const SMOOTH_COUNT: u8 = 2;

pub struct Schedule {
    path: PathBuf,
    lock_path: Option<PathBuf>, // 此方案可以锁定频率上限和下限，优先级高
    pub target_diff: Arc<Atomic<Cycles>>,
    pub max_diff: Arc<Atomic<Cycles>>,
    pub cur_freq: Arc<Atomic<Cycles>>,
    pool: WritePool,
    // touch boost
    touch_listener: Option<TouchListener>,
    touch_timer: Instant,
    touch_conf: (usize, usize, Duration),
    // freq pos
    pos: usize,
    smooth: SMA, // 均值平滑频率索引，缓解抖动
    smoothed_pos: usize,
    burst: usize,
    table: Vec<Cycles>,
}

impl Schedule {
    #[allow(clippy::cast_precision_loss)]
    pub fn new(path: &Path, config: &Config) -> Result<Self> {
        let target_diff = Arc::new(Atomic::new(Cycles::from_mhz(200)));

        let lock_path = Path::new("/proc/cpudvfs/cpufreq_debug");
        let lock_path = if lock_path.exists() {
            info!("Freq force bound is supported");
            Some(lock_path.to_owned())
        } else {
            None
        };

        let count = fs::read_to_string(path.join("affected_cpus"))
            .unwrap()
            .split_whitespace()
            .count();
        let pool = WritePool::new(cmp::max(count / 2, 2));

        let mut table: Vec<Cycles> = fs::read_to_string(path.join("scaling_available_frequencies"))
            .unwrap()
            .split_whitespace()
            .map(|freq| Cycles::from_khz(freq.parse().unwrap()))
            .collect();

        table.sort_unstable();

        let max_diff = Arc::new(Atomic::new(table.last().copied().unwrap()));
        let cur_freq = Arc::new(Atomic::new(table.last().copied().unwrap()));

        debug!("Got cpu freq table: {:#?}", &table);

        let pos = table.len() - 1;

        Ok(Self {
            path: path.to_owned(),
            lock_path,
            target_diff,
            max_diff,
            cur_freq,
            touch_listener: TouchListener::new(5).ok(),
            touch_timer: Instant::now(),
            burst: BURST_DEFAULT,
            pool,
            smooth: SMA::new(SMOOTH_COUNT, &(pos as f64)).unwrap(),
            table,
            smoothed_pos: pos,
            pos,
            touch_conf: Self::touch_boost_conf(config)?,
        })
    }

    pub fn run(&mut self, diff: Cycles) {
        if diff < Cycles::new(0) {
            return;
        }

        let target_diff = self.target_diff.load(Ordering::Acquire);
        let target_diff = target_diff.min(self.max_diff.load(Ordering::Acquire));

        assert!(
            target_diff.as_hz() >= 0,
            "Target diff should never be less than zero, but got {target_diff}"
        );

        trace!(
            "Schedutiling {} with target diff: {target_diff}",
            self.path.file_name().and_then(OsStr::to_str).unwrap()
        );

        match target_diff.cmp(&diff) {
            CmpOrdering::Less | CmpOrdering::Equal => {
                self.pos = self.pos.saturating_sub(1);
                self.burst = BURST_DEFAULT;
            }
            CmpOrdering::Greater => {
                self.pos = cmp::min(self.pos + self.burst, self.table.len() - 1);
                self.burst = cmp::min(BURST_MAX, self.burst + 1);
            }
        }

        self.smooth_pos(); // 更新SMA窗口
        self.write();
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn init(&mut self) {
        self.burst = 0;
        self.pos = self.table.len() - 1;
        self.smooth = SMA::new(SMOOTH_COUNT, &(self.pos as f64)).unwrap();
        let _ = self
            .pool
            .write(self.path.join("scaling_governor"), "performance");
        self.write();
    }
}
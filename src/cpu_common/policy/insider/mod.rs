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
mod event_loop;
mod misc;
mod normal;
mod utils;

use std::{
    cell::{Cell, RefCell},
    fs,
    path::{Path, PathBuf},
    sync::mpsc::Receiver,
    thread,
};

use anyhow::Result;

use super::super::Freq;

pub enum Event {
    InitDefault(bool),
    InitGame(bool),
    SetFasFreq(Freq),
    SetFasGovernor(bool),
}

#[derive(Debug)]
pub struct Insider {
    cpus: Vec<i32>,
    path: PathBuf,
    cache: Cell<Freq>,
    freqs: Vec<Freq>,
    fas_boost: Cell<bool>,
    gov_snapshot: RefCell<Option<String>>,
    rx: Receiver<Event>,
}

impl Insider {
    pub fn spawn<P: AsRef<Path>>(rx: Receiver<Event>, p: P) -> Result<Vec<Freq>> {
        let path = p.as_ref();

        let mut freqs: Vec<Freq> = fs::read_to_string(path.join("scaling_available_frequencies"))?
            .split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect();
        freqs.sort_unstable();

        let cpus = fs::read_to_string(path.join("affected_cpus"))?;
        let mut cpus: Vec<i32> = cpus
            .split_whitespace()
            .map(|c| c.parse().unwrap())
            .collect();
        cpus.sort_unstable();

        let thread_name = format!("policy {}-{}", cpus[0], cpus.last().unwrap());
        let policy = Self {
            cpus,
            path: path.to_path_buf(),
            freqs: freqs.clone(),
            cache: Cell::new(0),
            fas_boost: Cell::new(false),
            gov_snapshot: RefCell::new(None),
            rx,
        };

        thread::Builder::new()
            .name(thread_name)
            .spawn(move || Self::event_loop(policy))?;

        Ok(freqs)
    }
}

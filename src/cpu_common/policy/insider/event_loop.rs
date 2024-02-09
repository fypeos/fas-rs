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
use std::{
    collections::HashMap,
    thread,
    time::{Duration, Instant},
};

use cpu_cycles_reader::{Cycles, CyclesInstant, CyclesReader};

use super::{Event, Insider};

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Fas,
    Normal,
}

impl Insider {
    pub fn event_loop(self) {
        let mut state = State::Normal;
        let reader = CyclesReader::new().unwrap();
        let mut cycles: HashMap<i32, CyclesInstant> = HashMap::with_capacity(self.cpus.len());
        let mut last_record = Instant::now();
        let mut userspace_governor = false;

        loop {
            if userspace_governor && state == State::Normal {
                thread::sleep(Duration::from_millis(25));
                let max_cycles = self.max_cycles_per_secs(&reader, &mut last_record, &mut cycles);
                self.normal_policy(max_cycles);
            }

            if let Some(event) = self.recv_event(state, userspace_governor) {
                match event {
                    Event::InitDefault(b) => {
                        state = State::Normal;
                        userspace_governor = b;
                        self.init_default(b)
                    }
                    Event::InitGame(b) => {
                        state = State::Fas;
                        self.init_game(b)
                    }
                    Event::SetFasFreq(f) => self.set_fas_freq(f),
                    Event::SetFasGovernor(b) => self.set_fas_governor(b),
                }
                .unwrap();
            }
        }
    }

    fn recv_event(&self, state: State, userspace_governor: bool) -> Option<Event> {
        match state {
            State::Fas => self.rx.recv().ok(),
            State::Normal => {
                if userspace_governor {
                    self.rx.try_recv().ok()
                } else {
                    self.rx.recv().ok()
                }
            }
        }
    }

    fn max_cycles_per_secs(
        &self,
        reader: &CyclesReader,
        last_record: &mut Instant,
        map: &mut HashMap<i32, CyclesInstant>,
    ) -> Cycles {
        let mut cycles = Cycles::ZERO;
        for cpu in self.cpus.iter().copied() {
            let now = reader.instant(cpu).unwrap();
            let prev = map.entry(cpu).or_insert(now);

            cycles = cycles.max(now - *prev);
            *prev = now;
        }

        let time = last_record.elapsed();
        *last_record = Instant::now();

        let rhs = 1.0 / time.as_secs_f64();
        cycles.mul_f64(rhs)
    }
}

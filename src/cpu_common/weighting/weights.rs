// Copyright 2023 shadow3aaa@gitbub.com
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{cmp, collections::HashMap};

use crate::{cpu_common::cpu_info::Info, Mode};

#[derive(Debug)]
pub struct Weights {
    pub map: HashMap<Vec<i32>, f64>,
}

impl Weights {
    pub fn new(policys: &[Info]) -> Self {
        let map = policys
            .iter()
            .map(|policy| (policy.cpus.clone(), 0.0))
            .collect();
        Self { map }
    }

    pub fn weight(&self, cpus: &Vec<i32>, mode: Mode) -> f64 {
        self.amplify_probabilities_log(mode)
            .get(cpus)
            .unwrap()
            .min(0.8)
            + 1.0
    }

    fn amplify_probabilities_log(&self, mode: Mode) -> HashMap<Vec<i32>, f64> {
        let mut map = self.map.clone();
        let epsilon = 1e-10;
        let mut log_sum = 0.0;

        let factor = match mode {
            Mode::Powersave => 6.3,
            Mode::Balance => 6.0,
            Mode::Performance => 5.9,
            Mode::Fast => 5.8,
        };

        // Apply the logarithmic transformation and multiply by the amplification factor
        for weight in map.values_mut() {
            *weight = (*weight + epsilon).ln() * factor;
            log_sum += (*weight).exp();
        }

        // Normalize
        for weight in map.values_mut() {
            *weight = (*weight).exp() / log_sum;
        }

        let min = map
            .values()
            .min_by(|weight_a, weight_b| {
                weight_a
                    .partial_cmp(weight_b)
                    .unwrap_or(cmp::Ordering::Equal)
            })
            .copied()
            .unwrap_or(0.0);
        map.values_mut().for_each(|weight| *weight -= min);

        map
    }
}

use std::fmt::Display;
use std::sync::atomic::Ordering::Relaxed;
use std::{collections::BTreeMap, sync::atomic::AtomicUsize};

use ascii_table::AsciiTable;
use itertools::Itertools;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, LogNormal};

#[derive(Debug, Clone, Copy)]
enum HedgingStrategy {
    /// Don't try anything
    None,
    /// Send every request N times
    Immediate(usize),
    /// Hedge requests if the take more than a predetermined time
    Delayed(f64),
    /// Hedge a certain number of requests
    Random(f64),
}

impl std::fmt::Display for HedgingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HedgingStrategy::None => write!(f, "No hedging"),
            HedgingStrategy::Immediate(n) => write!(f, "Hedge {n} requests"),
            HedgingStrategy::Delayed(delay) => write!(f, "Wait {delay}ms before hedging"),
            HedgingStrategy::Random(chance) => write!(f, "Randomly hedge {chance}%"),
        }
    }
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);
const RNG_SEED: u64 = 0;
const SAMPLE_SIZE: usize = 100_000;

impl HedgingStrategy {
    fn make_request(&self, distribution: &LogNormal<f64>, rng: &mut StdRng) -> f64 {
        match self {
            HedgingStrategy::None => {
                COUNTER.fetch_add(1, Relaxed);
                distribution.sample(&mut *rng)
            }
            HedgingStrategy::Immediate(n) => {
                let mut sample = f64::MAX;
                for _ in 0..usize::max(*n, 1) {
                    COUNTER.fetch_add(1, Relaxed);
                    let new = distribution.sample(&mut *rng);
                    sample = sample.min(new);
                }

                sample
            }
            HedgingStrategy::Delayed(delay) => {
                COUNTER.fetch_add(1, Relaxed);
                let baseline = distribution.sample(&mut *rng);
                if baseline > *delay {
                    COUNTER.fetch_add(1, Relaxed);
                    f64::min(baseline, distribution.sample(&mut *rng))
                } else {
                    baseline
                }
            }
            HedgingStrategy::Random(chance) => {
                COUNTER.fetch_add(1, Relaxed);
                let baseline = distribution.sample(&mut *rng);

                if rng.random_bool(*chance) {
                    COUNTER.fetch_add(1, Relaxed);
                    f64::min(baseline, distribution.sample(&mut *rng))
                } else {
                    baseline
                }
            }
        }
    }
}

#[derive(Debug, Default)]
struct Percentiles {
    inner: BTreeMap<usize, f64>,
}

impl Percentiles {
    fn insert(&mut self, percentile: usize, value: f64) {
        if percentile > 100 {
            panic!("Percentiles must be between 0 and 100 (inclusive).");
        }
        self.inner.insert(percentile, value);
    }
}

impl FromIterator<(usize, f64)> for Percentiles {
    fn from_iter<T: IntoIterator<Item = (usize, f64)>>(iter: T) -> Self {
        Self {
            inner: BTreeMap::from_iter(iter),
        }
    }
}

impl IntoIterator for Percentiles {
    type Item = (usize, f64);

    type IntoIter = std::collections::btree_map::IntoIter<usize, f64>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl std::fmt::Display for Percentiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self
            .inner
            .iter()
            .map(|(k, v)| format!("p{k} - {v}"))
            .join(", ");
        write!(f, "{s}")
    }
}

fn main() {
    let distribution = LogNormal::new(4.7, 0.5).unwrap(); //p50 ~ 100, p95 ~ 250 and p100 ~ 600
    let mut rng = StdRng::seed_from_u64(RNG_SEED);

    let mut ascii_table = AsciiTable::default();
    ascii_table.column(0).set_header("Strategy");
    ascii_table.column(1).set_header("Overhead");
    ascii_table.column(2).set_header("p50");
    ascii_table.column(3).set_header("p75");
    ascii_table.column(4).set_header("p90");
    ascii_table.column(5).set_header("p95");
    ascii_table.column(6).set_header("p99");
    ascii_table.column(7).set_header("p100");

    let mut table_data: Vec<Vec<Box<dyn Display>>> = vec![];

    for strategy in vec![
        HedgingStrategy::None,
        HedgingStrategy::Immediate(2),
        HedgingStrategy::Delayed(250.0),
        HedgingStrategy::Random(0.05),
    ] {
        let mut row = vec![Box::new(strategy) as _];
        let percentiles = for_strategy(strategy, &distribution, &mut rng);

        let counter = COUNTER.fetch_update(Relaxed, Relaxed, |_| Some(0)).unwrap();
        row.push(Box::new(counter - SAMPLE_SIZE) as _);

        row.extend(
            percentiles
                .into_iter()
                .map(|(_k, v)| Box::new(format!("{v:.2}")) as _),
        );

        table_data.push(row);
    }

    ascii_table.print(table_data);
}

fn for_strategy(
    strategy: HedgingStrategy,
    distribution: &LogNormal<f64>,
    rng: &mut StdRng,
) -> Percentiles {
    let mut samples = Vec::with_capacity(SAMPLE_SIZE);
    for _ in 0..SAMPLE_SIZE {
        samples.push(strategy.make_request(distribution, rng));
    }

    calculate_percentiles(samples)
}

fn calculate_percentiles(mut samples: Vec<f64>) -> Percentiles {
    samples.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());

    let mut percentiles = Percentiles::default();

    percentiles.insert(50, samples[samples.len() / 2]);
    percentiles.insert(75, samples[75 * samples.len() / 100]);
    percentiles.insert(90, samples[90 * samples.len() / 100]);
    percentiles.insert(90, samples[95 * samples.len() / 100]);
    percentiles.insert(99, samples[99 * samples.len() / 100]);
    percentiles.insert(100, samples[samples.len() - 1]);

    percentiles
}

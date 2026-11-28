//! Closed-form emission. All times are seconds, and rates are births/second.
#[derive(Clone, Debug, Default)]
pub(super) struct Schedule {
    rates: Vec<RateKey>,
    trains: Vec<Train>,
    // In each subtree, the latest possible birth (including jitter). A long
    // pulse train must not force a seek to scan all intervening expired bursts.
    last_birth: Vec<f64>,
}

#[derive(Clone, Copy, Debug, Default)]
struct RateKey {
    time: f64,
    rate: f64,
    integral: f64,
    incoming: Option<f64>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Train {
    pub start: f64,
    pub period: f64,
    pub groups: u32,
    pub count: u32,
    pub jitter: f64,
    pub track: u64,
}

#[derive(Clone, Copy)]
pub(super) struct Birth {
    pub time: f64,
    pub track: u64,
    pub group: u64,
    pub count: u32,
}

pub(super) fn hash(s: &str) -> u64 {
    s.bytes()
        .fold(0xcbf29ce484222325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3))
}

fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

pub(super) fn unit(seed: u64, track: u64, group: u64, index: u32, channel: u64) -> f64 {
    let batch = mix(mix(seed ^ track) ^ group);
    let particle = mix(batch ^ u64::from(index));
    let bits = mix(particle ^ channel);
    (bits >> 11) as f64 / ((1u64 << 53) as f64)
}

impl Schedule {
    pub fn train(&mut self, train: Train) -> Result<(), &'static str> {
        let i = self.trains.partition_point(|p| p.start < train.start);
        if self.trains[i..]
            .iter()
            .take_while(|p| p.start == train.start)
            .any(|p| p.track == train.track)
        {
            return Err("simultaneous particle calls need distinct track names");
        }
        self.trains.insert(i, train);
        let capacity = self.trains.len().next_power_of_two();
        let last = |row: &Train| row.start + f64::from(row.groups - 1) * row.period + row.jitter;
        if self.last_birth.len() != 2 * capacity || i + 1 != self.trains.len() {
            self.last_birth.resize(2 * capacity, f64::NEG_INFINITY);
            self.last_birth[capacity..].fill(f64::NEG_INFINITY);
            for (leaf, row) in self.last_birth[capacity..].iter_mut().zip(&self.trains) {
                *leaf = last(row);
            }
            for node in (1..capacity).rev() {
                self.last_birth[node] =
                    self.last_birth[node * 2].max(self.last_birth[node * 2 + 1]);
            }
        } else {
            let mut node = capacity + i;
            self.last_birth[node] = last(&train);
            while node > 1 {
                node /= 2;
                self.last_birth[node] =
                    self.last_birth[node * 2].max(self.last_birth[node * 2 + 1]);
            }
        }
        Ok(())
    }

    fn visit_trains(
        &self,
        node: usize,
        start: usize,
        width: usize,
        lo: f64,
        t: f64,
        seed: u64,
        draw: &mut impl FnMut(Birth) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        if self.last_birth[node] <= lo || start >= self.trains.len() || self.trains[start].start > t
        {
            return Ok(());
        }
        if width > 1 {
            let half = width / 2;
            self.visit_trains(node * 2, start, half, lo, t, seed, draw)?;
            self.visit_trains(node * 2 + 1, start + half, half, lo, t, seed, draw)?;
            return Ok(());
        }
        let train = &self.trains[start];
        let first = ((lo - train.start - train.jitter) / train.period).floor().max(0.0) as u32;
        let last = (((t - train.start + train.jitter) / train.period).floor() as u32)
            .saturating_add(1)
            .min(train.groups);
        for g in first..last {
            let offset = (unit(seed, train.track, g.into(), 0, 0) * 2.0 - 1.0) * train.jitter;
            let time = (train.start + f64::from(g) * train.period + offset).max(train.start);
            if time > lo && time <= t {
                draw(Birth {
                    time,
                    track: train.track,
                    group: g.into(),
                    count: train.count,
                })?;
            }
        }
        Ok(())
    }

    pub(super) fn end_hint(&self, life: f64) -> f64 {
        let last_train = self.last_birth.get(1).copied();
        let last_rate = self
            .rates
            .last()
            .filter(|_| {
                self.rates
                    .iter()
                    .any(|key| key.rate > 0.0 || key.incoming.is_some_and(|r| r > 0.0))
            })
            .map(|key| key.time);
        last_train
            .into_iter()
            .chain(last_rate)
            .reduce(f64::max)
            .map_or(0., |t| t + life)
    }

    fn segment_at(&self, time: f64) -> (f64, Option<f64>) {
        let i = self.rates.partition_point(|k| k.time < time);
        if let Some(k) = self.rates.get(i).filter(|k| k.time == time) {
            return (k.rate, k.incoming);
        }
        let Some(prev) = i.checked_sub(1).map(|j| self.rates[j]) else {
            return (0., None);
        };
        let Some((next, end_rate)) = self.rates.get(i).and_then(|k| k.incoming.map(|r| (k, r)))
        else {
            return (prev.rate, None);
        };
        let value =
            prev.rate + (end_rate - prev.rate) * (time - prev.time) / (next.time - prev.time);
        (value, Some(value))
    }

    fn key(&mut self, time: f64, rate: f64, incoming: Option<f64>) {
        let i = self.rates.partition_point(|k| k.time < time);
        let key = RateKey {
            time,
            rate,
            incoming,
            integral: 0.,
        };
        if self.rates.get(i).is_some_and(|k| k.time == time) {
            self.rates[i] = key;
        } else {
            self.rates.insert(i, key);
        }
        for j in i.max(1)..self.rates.len() {
            let prev = self.rates[j - 1];
            let k = &mut self.rates[j];
            let mean = k.incoming.map_or(prev.rate, |end| (prev.rate + end) * 0.5);
            k.integral = prev.integral + (k.time - prev.time) * mean;
        }
    }

    pub fn rate(&mut self, start: f64, end: f64, rate: f64) {
        let (from, incoming) = self.segment_at(start);
        if start < end {
            self.key(start, from, incoming);
            self.key(end, rate, Some(rate));
        } else {
            self.key(start, rate, incoming);
        }
    }

    /// Visit the half-open birth window (t - life, t]. Invert the integral
    /// inside each rate segment so a seek never enumerates earlier births.
    pub fn batches(
        &self,
        t: f64,
        life: f64,
        seed: u64,
        jitter: f64,
    ) -> Result<Vec<Birth>, &'static str> {
        const MAX_CANDIDATES: u32 = 100_000;
        let mut remaining = MAX_CANDIDATES;
        let mut batches = Vec::new();
        let mut draw = |birth: Birth| {
            remaining = remaining.checked_sub(birth.count).ok_or("particle birth window exceeds 100,000 candidates per emitter; reduce rate, batch size or lifetime")?;
            batches.push(birth);
            Ok(())
        };
        let lo = t - life;
        if !self.trains.is_empty() {
            self.visit_trains(1, 0, self.last_birth.len() / 2, lo, t, seed, &mut draw)?;
        }
        let start = self.rates.partition_point(|k| k.time <= lo - jitter).saturating_sub(1);
        for i in start..self.rates.len() {
            let k = self.rates[i];
            if k.time > t + jitter {
                break;
            }
            let next = self.rates.get(i + 1);
            let end = next.map_or(f64::INFINITY, |n| n.time);
            let slope =
                next.and_then(|n| n.incoming).map_or(0.0, |r| (r - k.rate) / (end - k.time));
            if k.rate == 0.0 && slope == 0.0 {
                continue;
            }
            let area = |x: f64| k.integral + (k.rate + 0.5 * slope * x) * x;
            let a = (lo - jitter - k.time).max(0.0);
            let b = (t + jitter).min(end) - k.time;
            if b < a {
                continue;
            }
            for group in (area(a).floor() as u64)..=(area(b).ceil() as u64) {
                let mass = group as f64 - k.integral;
                if mass < 0.0 {
                    continue;
                }
                let discr = k.rate * k.rate + 2.0 * slope * mass;
                if discr < 0.0 {
                    continue;
                }
                let dt = if mass == 0.0 {
                    0.0
                } else {
                    2.0 * mass / (k.rate + discr.sqrt())
                };
                if k.time + dt >= end {
                    continue;
                }
                let time = (k.time + dt + (unit(seed, STREAM, group, 0, 0) * 2.0 - 1.0) * jitter)
                    .clamp(k.time, end.next_down());
                if time > lo && time <= t && time < end {
                    draw(Birth {
                        time,
                        track: STREAM,
                        group,
                        count: 1,
                    })?;
                }
            }
        }
        Ok(batches)
    }
}
const STREAM: u64 = 0xb08a4d9130ae781f;

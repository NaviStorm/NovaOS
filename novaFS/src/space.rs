//! Partition de l'espace non possédé, tenue en intervalles bornés.
//! Âge zéro = libre ; un âge positif conserve une génération encore protégée.
use crate::{Error, Result};

pub(crate) const MAX_RUNS: usize = 4096;
#[derive(Clone, Copy, Debug)]
pub(crate) struct Run {
    pub start: u64,
    pub end: u64,
    pub age: u64,
}

#[derive(Default)]
pub(crate) struct Space(pub Vec<Run>);

impl Space {
    pub fn fork(&self) -> Result<Self> {
        let mut runs = Vec::new();
        runs.try_reserve_exact(self.0.len())
            .map_err(|_| Error::NoMemory)?;
        runs.extend_from_slice(&self.0);
        Ok(Self(runs))
    }
    fn room(&mut self) -> Result<()> {
        if self.0.len() >= MAX_RUNS {
            return Err(Error::Limit("fragmentation de l’espace"));
        }
        if self.0.len() == self.0.capacity() {
            let growth = self.0.len().max(8).min(MAX_RUNS - self.0.len());
            self.0
                .try_reserve_exact(growth)
                .map_err(|_| Error::NoMemory)?;
        }
        Ok(())
    }
    pub fn fresh(blocks: u64) -> Self {
        Self(vec![Run {
            start: 2,
            end: blocks,
            age: 0,
        }])
    }
    pub fn decoded(&mut self, key: &[u8], val: &[u8], blocks: u64, g: u64) -> Result<()> {
        let start = u64::from_le_bytes(key.try_into().map_err(|_| Error::Corrupt("clé espace"))?);
        if val.len() != 17 || start < 2 {
            return Err(Error::Corrupt("format espace"));
        }
        let len = u64::from_le_bytes(val[..8].try_into().unwrap());
        let age = u64::from_le_bytes(val[9..].try_into().unwrap());
        let end = start
            .checked_add(len)
            .ok_or(Error::Corrupt("longueur espace"))?;
        if end > blocks
            || len == 0
            || val[8] != u8::from(age != 0)
            || age >= g
            || self.0.last().is_some_and(|r| r.end > start)
        {
            return Err(Error::Corrupt("intervalle espace"));
        }
        self.room()?;
        self.0.push(Run { start, end, age });
        Ok(())
    }
    pub fn lookup(&self, block: u64) -> Option<Run> {
        let i = self.0.partition_point(|r| r.start <= block);
        i.checked_sub(1)
            .map(|j| self.0[j])
            .filter(|r| block < r.end)
    }
    pub fn eligible(&self, oldest: u64) -> u64 {
        self.0
            .iter()
            .filter(|r| r.age < oldest)
            .map(|r| r.end - r.start)
            .sum()
    }
    pub fn advance(&mut self, oldest: u64) {
        for r in &mut self.0 {
            if r.age < oldest {
                r.age = 0
            }
        }
        let mut count = 0;
        for i in 0..self.0.len() {
            let r = self.0[i];
            if count > 0 && self.0[count - 1].end == r.start && self.0[count - 1].age == r.age {
                self.0[count - 1].end = r.end;
            } else {
                self.0[count] = r;
                count += 1;
            }
        }
        self.0.truncate(count);
    }
    pub fn take(&mut self) -> Result<u64> {
        let i = self
            .0
            .iter()
            .position(|r| r.age == 0)
            .ok_or(Error::NoSpace)?;
        let b = self.0[i].start;
        self.0[i].start += 1;
        if self.0[i].start == self.0[i].end {
            self.0.remove(i);
        }
        Ok(b)
    }
    pub fn release(&mut self, b: u64, age: u64, blocks: u64) -> Result<()> {
        if !(2..blocks).contains(&b) || self.lookup(b).is_some() {
            return Err(Error::Corrupt("double libération ou bloc hors LV"));
        }
        let i = self.0.partition_point(|r| r.start < b);
        let left = i > 0 && self.0[i - 1].end == b && self.0[i - 1].age == age;
        let right = i < self.0.len() && self.0[i].start == b + 1 && self.0[i].age == age;
        if left {
            self.0[i - 1].end = if right { self.0.remove(i).end } else { b + 1 };
        } else if right {
            self.0[i].start = b;
        } else {
            self.room()?;
            self.0.insert(
                i,
                Run {
                    start: b,
                    end: b + 1,
                    age,
                },
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intervalles_fragmentes_comparees_a_un_modele_par_bloc() {
        let n = 1000u64;
        let mut space = Space::fresh(n);
        let mut state = vec![Some(0); n as usize];
        state[..2].fill(None);
        let mut seed = 7832u64;
        for step in 0..10000u64 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let addr = 2 + seed % (n - 2);
            let oldest = (step / 100).max(1);
            if seed & 1 == 0 {
                let expected = (2..n as usize).find(|&i| state[i] == Some(0));
                assert_eq!(space.take().ok(), expected.map(|i| i as u64));
                if let Some(i) = expected {
                    state[i] = None;
                }
            } else if state[addr as usize].is_none() {
                let age = oldest + 1;
                space.release(addr, age, n).unwrap();
                state[addr as usize] = Some(age);
            }
            if step % 100 == 0 {
                space.advance(oldest);
                for age in state.iter_mut().flatten() {
                    if *age < oldest {
                        *age = 0
                    }
                }
                for (i, age) in state.iter().enumerate() {
                    assert_eq!(space.lookup(i as u64).map(|r| r.age), *age);
                }
                assert_eq!(
                    space.eligible(1),
                    state.iter().filter(|v| **v == Some(0)).count() as u64
                );
            }
        }
    }
}

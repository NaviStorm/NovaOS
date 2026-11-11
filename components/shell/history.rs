use alloc::{collections::VecDeque, string::String};

pub struct History {
    entries: VecDeque<String>,
    capacity: usize,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity,
        }
    }

    pub fn configured() -> nova_cli::runtime::Result<Self> {
        let capacity = option_env!("NOVA_HISTORY_ENTRIES").unwrap_or("64");
        Ok(Self::new(
            capacity
                .parse()
                .map_err(|_| nova_cli::runtime::BAD_ARGUMENT)?,
        ))
    }

    pub fn remember(&mut self, text: &str) {
        if self.capacity == 0
            || text.trim().is_empty()
            || self.entries.back().is_some_and(|s| s == text)
        {
            return;
        }
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(text.into());
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn get(&self, pos: usize) -> Option<&String> {
        self.entries.get(pos)
    }
}

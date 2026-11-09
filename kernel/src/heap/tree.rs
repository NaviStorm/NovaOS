//! Arbres binaires de blocs contigus. Les racines correspondent aux bits à 1
//! de la longueur engagée, qui peut ainsi croître d’une seule page à la fois.
use super::{PAGE_BYTES, QUANTUM};

const FREE: u8 = 0;
const LIVE: u8 = 1;
const FULL: u8 = 2; // Bloc divisé, sans enfant disponible.

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Block {
    pub start: usize,
    pub size: usize,
}

impl Block {
    fn left(self) -> Self {
        Self {
            start: self.start,
            size: self.size / 2,
        }
    }
    fn right(self) -> Self {
        Self {
            start: self.start + self.size / 2,
            size: self.size / 2,
        }
    }
    fn parent(self) -> Self {
        let size = self.size * 2;
        Self {
            start: self.start & !(size - 1),
            size,
        }
    }
    // Numérotation infixe : les feuilles ont des indices pairs. L’indice d’un
    // bloc reste stable lorsque de nouvelles pages sont ajoutées à droite.
    fn index(self) -> usize {
        (2 * self.start + self.size) / QUANTUM - 1
    }
}

fn roots(bytes: usize) -> impl Iterator<Item = Block> {
    let mut start = 0;
    core::iter::from_fn(move || {
        let remaining = bytes - start;
        if remaining == 0 {
            return None;
        }
        let size = 1usize << remaining.ilog2();
        let root = Block { start, size };
        start += size;
        Some(root)
    })
}

pub(super) struct Tree<'a> {
    pub nodes: &'a mut [u8],
    pub bytes: usize,
}

impl Tree<'_> {
    fn state(&self, b: Block) -> u8 {
        self.nodes[b.index()]
    }
    fn set(&mut self, b: Block, state: u8) {
        self.nodes[b.index()] = state;
    }

    fn largest(&self, b: Block) -> u8 {
        match self.state(b) {
            FREE => b.size.ilog2() as u8,
            LIVE | FULL => 0,
            order if order >= QUANTUM.ilog2() as u8 && order < b.size.ilog2() as u8 => order,
            _ => super::global::fatal(super::Error::Metadata),
        }
    }

    fn combine(&mut self, parent: Block) {
        let (a, b) = (parent.left(), parent.right());
        let state = if self.state(a) == FREE && self.state(b) == FREE {
            FREE
        } else {
            self.largest(a).max(self.largest(b)).max(FULL)
        };
        self.set(parent, state);
    }

    fn update(&mut self, mut b: Block, root: Block) {
        while b != root {
            b = b.parent();
            self.combine(b);
        }
    }

    pub fn extend(&mut self, previous: usize) {
        for start in (previous..self.bytes).step_by(PAGE_BYTES) {
            let mut b = Block {
                start,
                size: PAGE_BYTES,
            };
            self.set(b, FREE);
            // Le nouveau bloc et son jumeau gauche forment une nouvelle racine.
            while b.start & b.size != 0 {
                b = b.parent();
                self.combine(b);
            }
        }
    }

    pub fn has_space(&self, size: usize) -> bool {
        roots(self.bytes).any(|b| self.largest(b) >= size.ilog2() as u8)
    }

    pub fn take(&mut self, size: usize) -> Option<Block> {
        let order = size.ilog2() as u8;
        let root = roots(self.bytes).find(|&b| self.largest(b) >= order)?;
        let mut b = root;
        while b.size > size {
            if self.state(b) == FREE {
                self.set(b.left(), FREE);
                self.set(b.right(), FREE);
            }
            b = if self.largest(b.left()) >= order {
                b.left()
            } else {
                b.right()
            };
        }
        if self.state(b) != FREE {
            super::global::fatal(super::Error::Metadata);
        }
        self.set(b, LIVE);
        self.update(b, root);
        Some(b)
    }

    /// N’examine que des enfants de blocs divisés : un ancien marqueur LIVE
    /// sous un bloc déjà fusionné ne peut jamais valider une seconde libération.
    pub fn locate(&self, offset: usize) -> Option<(Block, Block)> {
        let root = roots(self.bytes).find(|b| offset >= b.start && offset < b.start + b.size)?;
        let mut b = root;
        loop {
            match self.state(b) {
                FREE => return None,
                LIVE => return (offset == b.start).then_some((b, root)),
                _ if b.size == QUANTUM => return None,
                _ => {
                    b = if offset < b.right().start {
                        b.left()
                    } else {
                        b.right()
                    }
                }
            }
        }
    }

    pub fn release(&mut self, b: Block, root: Block) {
        self.set(b, FREE);
        self.update(b, root);
    }

    pub fn resize(&mut self, mut b: Block, root: Block, size: usize) -> Option<Block> {
        if size > root.size || b.start % size != 0 {
            return None;
        }
        // Vérifier tous les voisins avant de changer le premier nœud.
        while b.size < size {
            let parent = b.parent();
            if self.state(parent.right()) != FREE {
                return None;
            }
            b = parent;
        }
        while b.size > size {
            self.set(b.right(), FREE);
            b = b.left();
        }
        self.set(b, LIVE);
        self.update(b, root);
        Some(b)
    }

    pub fn used_end(&self) -> usize {
        let Some(mut b) = roots(self.bytes).filter(|&b| self.state(b) != FREE).last() else {
            return 0;
        };
        while self.state(b) != LIVE {
            if b.size <= QUANTUM || self.state(b) == FREE {
                super::global::fatal(super::Error::Metadata);
            }
            b = if self.state(b.right()) != FREE {
                b.right()
            } else {
                b.left()
            };
        }
        b.start + b.size
    }

    pub fn empty_first_page(&mut self) {
        self.set(
            Block {
                start: 0,
                size: PAGE_BYTES,
            },
            FREE,
        );
    }
}

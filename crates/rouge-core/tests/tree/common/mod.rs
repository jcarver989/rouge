pub(crate) fn edit(ids: &mut Vec<u32>, next: &mut u32, random: &mut Random) {
    let place = random.below(ids.len() + 1);
    match random.below(8) {
        0 => ids.reverse(),
        1 if ids.len() > 1 => {
            let (a, b) = (random.below(ids.len()), random.below(ids.len()));
            ids.swap(a, b);
        }
        2 => ids.truncate(place),
        3 => {
            ids.drain(..place);
        }
        4 if place < ids.len() => {
            ids.remove(place);
        }
        5 => ids.rotate_left(place),
        _ => {
            for _ in 0..=random.below(4) {
                ids.insert(place, *next);
                *next += 1;
            }
        }
    }
}

pub(crate) struct Random(pub(crate) u64);

impl Random {
    pub(crate) fn below(&mut self, limit: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as usize % limit
    }
}

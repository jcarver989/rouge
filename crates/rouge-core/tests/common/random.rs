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

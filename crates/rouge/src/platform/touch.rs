use rouge_core::TouchPhase;

#[derive(Debug, Default)]
pub struct PrimaryTouch(Option<u64>);

impl PrimaryTouch {
    pub fn input(&mut self, id: u64, phase: TouchPhase) -> Option<TouchPhase> {
        match phase {
            TouchPhase::Start if self.0.is_none() => {
                self.0 = Some(id);
                Some(phase)
            }
            TouchPhase::Move if self.0 == Some(id) => Some(phase),
            TouchPhase::End | TouchPhase::Cancel if self.0 == Some(id) => {
                self.0 = None;
                Some(phase)
            }
            _ => None,
        }
    }
}

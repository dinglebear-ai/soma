#[derive(Debug, Default)]
pub struct DriveState {
    pub next_seq: u64,
}

impl DriveState {
    pub fn next_sequence(&mut self) -> u64 {
        let current = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        current
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "each channel is masked to one byte"
    )]
    pub const fn rgb(hex: u32) -> Self {
        Self::new((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, u8::MAX)
    }

    #[must_use]
    pub const fn over(self, background: Self) -> Self {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "weighted u8 average fits in u8"
        )]
        const fn channel(foreground: u8, background: u8, alpha: u8) -> u8 {
            let foreground = foreground as u32 * alpha as u32;
            let background = background as u32 * (u8::MAX - alpha) as u32;
            ((foreground + background + 127) / 255) as u8
        }
        Self::new(
            channel(self.r, background.r, self.a),
            channel(self.g, background.g, self.a),
            channel(self.b, background.b, self.a),
            u8::MAX,
        )
    }

    #[must_use]
    pub const fn mix(self, background: Self, amount: u8) -> Self {
        Self::new(self.r, self.g, self.b, amount).over(background)
    }
}

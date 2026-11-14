pub const UI_FONT: &str = "Inter";
pub const CODE_FONT: &str = "Lilex";

const FACES: [&[u8]; 8] = [
    include_bytes!("../assets/fonts/inter/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/inter/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/inter/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/inter/Inter-Bold.ttf"),
    include_bytes!("../assets/fonts/lilex/Lilex-Regular.ttf"),
    include_bytes!("../assets/fonts/lilex/Lilex-Bold.ttf"),
    include_bytes!("../assets/fonts/lilex/Lilex-Italic.ttf"),
    include_bytes!("../assets/fonts/lilex/Lilex-BoldItalic.ttf"),
];

pub fn bundled_fonts() -> impl Iterator<Item = &'static [u8]> {
    FACES.into_iter()
}

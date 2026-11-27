use super::document::document;

pub(crate) const SCHEME: &str = "rouge-page";

#[derive(Clone, Debug)]
pub(crate) struct Inline {
    pub(crate) address: String,
    pub(crate) document: String,
}

impl Inline {
    pub(crate) fn new(html: &str, base_url: Option<&str>) -> Self {
        let document = document(html, base_url);
        let address = format!("{SCHEME}://{:016x}/", fingerprint(&document));
        Inline { address, document }
    }
}

fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

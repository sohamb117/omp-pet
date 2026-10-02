//! Exact pixel-frequency accent, excluding transparency and near-monochrome colors.
use std::collections::BTreeMap;
pub const FALLBACK: [u8; 3] = [163, 227, 196];
#[derive(Default)]
pub struct Palette(BTreeMap<[u8; 3], u64>);
impl Palette {
    pub fn add(&mut self, rgb: [u8; 3], alpha: u8) {
        let max = *rgb.iter().max().unwrap() as u16;
        let min = *rgb.iter().min().unwrap() as u16;
        if alpha < 128 || max < 24 || max - min < 32 || (max - min) * 100 < max * 25 {
            return;
        }
        *self.0.entry(rgb).or_default() += 1;
    }
    pub fn dominant(&self) -> [u8; 3] {
        self.0
            .iter()
            .max_by_key(|(_, count)| *count)
            .map_or(FALLBACK, |(rgb, _)| *rgb)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_common_black_gray_white_and_transparent_pixels() {
        let mut p = Palette::default();
        for _ in 0..1000 {
            p.add([0, 0, 0], 255);
            p.add([255, 255, 255], 255);
            p.add([40, 42, 46], 255);
            p.add([110, 111, 138], 255);
            p.add([255, 0, 0], 0);
        }
        for _ in 0..8 {
            p.add([192, 40, 64], 255);
        }
        for _ in 0..3 {
            p.add([48, 160, 220], 255);
        }
        assert_eq!(p.dominant(), [192, 40, 64]);
    }
}

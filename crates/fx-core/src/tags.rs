//! Color tags (task T3.4): up to 7 per item, kept as a bit mask.

/// (Portuguese name, English name, color RGBA) of each tag, in bit order.
pub const TAGS: [(&str, &str, u32); 7] = [
    ("vermelho", "red", 0xE5484DFF),
    ("laranja", "orange", 0xF76B15FF),
    ("amarelo", "yellow", 0xFFC53DFF),
    ("verde", "green", 0x30A46CFF),
    ("azul", "blue", 0x0090FFFF),
    ("roxo", "purple", 0x8E4EC6FF),
    ("cinza", "gray", 0x8B8D98FF),
];

/// The bit of a tag named in either language (or by its number 1-7).
pub fn bit_for(name: &str) -> Option<u8> {
    let n = name.trim().to_lowercase();
    if let Ok(k) = n.parse::<usize>()
        && (1..=TAGS.len()).contains(&k)
    {
        return Some(1 << (k - 1));
    }
    TAGS.iter()
        .position(|(pt, en, _)| *pt == n || *en == n)
        .map(|i| 1 << i)
}

/// The tags of a mask, in order: (index, color).
pub fn colors(mask: u8) -> impl Iterator<Item = (usize, u32)> {
    TAGS.iter()
        .enumerate()
        .filter(move |(i, _)| mask & (1 << i) != 0)
        .map(|(i, t)| (i, t.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_numbers_and_colors() {
        assert_eq!(bit_for("Verde"), Some(1 << 3));
        assert_eq!(bit_for("blue"), Some(1 << 4));
        assert_eq!(bit_for("1"), Some(1));
        assert_eq!(bit_for("8"), None);
        assert_eq!(bit_for("rosa"), None);
        let c: Vec<usize> = colors(0b100_0001).map(|(i, _)| i).collect();
        assert_eq!(c, [0, 6]);
    }
}

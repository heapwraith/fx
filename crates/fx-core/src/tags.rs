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

/// Names the user gave to the tags (T5.5), in bit order; `None`: the
/// built-in name. Set once the saved names load and on every rename.
static CUSTOM: std::sync::RwLock<[Option<String>; 7]> =
    std::sync::RwLock::new([None, None, None, None, None, None, None]);

/// Gives tag `k` its own name (`None` or empty: back to the built-in one).
pub fn set_name(k: usize, name: Option<String>) {
    let name = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty());
    if let Some(slot) = CUSTOM
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_mut(k)
    {
        *slot = name;
    }
}

/// The name of tag `k`: the user's, else the built-in one in the
/// interface language.
pub fn name(k: usize) -> String {
    let custom = CUSTOM
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(k)
        .cloned()
        .flatten();
    custom.unwrap_or_else(|| {
        let (pt, en, _) = TAGS[k.min(TAGS.len() - 1)];
        if crate::locale::english() { en } else { pt }.to_owned()
    })
}

/// The bit of a tag named in either language, by the user's name for it
/// (case ignored), or by its number 1-7.
pub fn bit_for(name: &str) -> Option<u8> {
    let n = name.trim().to_lowercase();
    if let Ok(k) = n.parse::<usize>()
        && (1..=TAGS.len()).contains(&k)
    {
        return Some(1 << (k - 1));
    }
    let custom = CUSTOM
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .position(|c| c.as_deref().is_some_and(|c| c.to_lowercase() == n));
    custom
        .or_else(|| TAGS.iter().position(|(pt, en, _)| *pt == n || *en == n))
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

    #[test]
    fn custom_names_are_found_and_shown() {
        set_name(5, Some(" Trabalho ".into()));
        assert_eq!(name(5), "Trabalho");
        assert_eq!(bit_for("trabalho"), Some(1 << 5));
        assert_eq!(
            bit_for("roxo"),
            Some(1 << 5),
            "the built-in name still works"
        );
        set_name(5, Some(String::new()));
        assert_eq!(bit_for("trabalho"), None);
        assert!(name(5) == "roxo" || name(5) == "purple");
    }
}

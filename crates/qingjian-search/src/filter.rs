//! 把搜索建议筛成能挂到当前拼音上的候选词。
//!
//! 搜索接口给的是「搜索热词」，其中很多是短语（`伊斯塔露 原神`、`伊斯塔露cos`），不能整段上屏。
//! 只留整条都是汉字、字数又正好等于音节数的那些：它们才是这段拼音可能对应的词。

use qingjian_core::CloudWord;

/// 建议词 → 候选词。音节直接取用户敲的那次切分——搜索接口不给读音，用输入拼音当它的读音，
/// 上屏时才能按音节消耗拼音、也才能记成用户词。`max` 是条数上限。
pub fn to_cloud_words(suggestions: Vec<String>, pinyin: &str, max: usize) -> Vec<CloudWord> {
    if pinyin.trim().is_empty() || max == 0 {
        return Vec::new();
    }
    let syllables: Vec<String> = pinyin.split('\'').map(str::to_owned).collect();
    suggestions
        .into_iter()
        .filter(|text| is_all_han(text) && text.chars().count() == syllables.len())
        .take(max)
        .map(|text| CloudWord {
            text,
            syllables: syllables.clone(),
            reading: None,
        })
        .collect()
}

/// 整条都是汉字（CJK 统一表意文字及扩展 A、兼容表意文字）。带空格、字母、数字的一律不要。
fn is_all_han(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(words: &[CloudWord]) -> Vec<&str> {
        words.iter().map(|w| w.text.as_str()).collect()
    }

    #[test]
    fn keeps_only_words_that_match_the_syllable_count() {
        let suggestions = vec![
            "伊斯塔露".to_owned(),      // 4 字，4 音节：留
            "伊斯塔露是谁".to_owned(),  // 6 字：丢
            "伊斯塔露 原神".to_owned(), // 带空格：丢
            "伊斯塔露cos".to_owned(),   // 带字母：丢
            "原神".to_owned(),          // 2 字：丢
        ];
        let words = to_cloud_words(suggestions, "yi'si'ta'lu", 4);
        assert_eq!(texts(&words), ["伊斯塔露"]);
        assert_eq!(words[0].syllables, ["yi", "si", "ta", "lu"]);
    }

    #[test]
    fn truncates_to_the_limit() {
        let suggestions = vec![
            "甲乙丙".to_owned(),
            "丁戊己".to_owned(),
            "庚辛壬".to_owned(),
        ];
        assert_eq!(
            texts(&to_cloud_words(suggestions, "jia'yi'bing", 2)).len(),
            2
        );
    }

    #[test]
    fn zero_limit_or_empty_pinyin_yields_nothing() {
        let suggestions = vec!["甲乙丙".to_owned()];
        assert!(to_cloud_words(suggestions.clone(), "jia'yi'bing", 0).is_empty());
        assert!(to_cloud_words(suggestions, "", 2).is_empty());
    }
}

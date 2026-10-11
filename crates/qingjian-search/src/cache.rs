//! 最近几次查询的建议词。退格再打回同样的拼音很常见，不用再花一次请求。

use std::collections::{HashMap, VecDeque};

use qingjian_core::{CloudWord, PredictionRequest};

/// 最近若干次查询的建议词。
#[derive(Debug, Default)]
pub struct SuggestionCache {
    /// 请求内容 → 建议词。
    entries: HashMap<String, Vec<CloudWord>>,

    /// 插入顺序，用来淘汰最旧的。
    order: VecDeque<String>,

    /// 容量。
    capacity: usize,
}

impl SuggestionCache {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            capacity,
        }
    }

    /// 序号不参与：同一个拼音与市场就是同一个请求。键用带 `'` 的切分，
    /// `xian` 与 `xi'an` 敲的字母一样、切分不同，结果不该互相命中。
    pub fn key(request: &PredictionRequest, market: &str) -> String {
        format!("{}\u{1}{market}", request.pinyin)
    }

    pub fn get(&self, key: &str) -> Option<&Vec<CloudWord>> {
        self.entries.get(key)
    }

    pub fn insert(&mut self, key: String, words: Vec<CloudWord>) {
        if self.entries.insert(key.clone(), words).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str) -> CloudWord {
        CloudWord {
            text: text.to_owned(),
            syllables: vec!["a".to_owned()],
            reading: None,
        }
    }

    #[test]
    fn evicts_oldest_beyond_capacity() {
        let mut cache = SuggestionCache::with_capacity(2);
        cache.insert("a".into(), vec![word("甲")]);
        cache.insert("b".into(), vec![word("乙")]);
        cache.insert("a".into(), vec![word("丙")]);
        cache.insert("c".into(), vec![word("丁")]);
        assert!(cache.get("a").is_none());
        assert_eq!(cache.get("b").unwrap()[0].text, "乙");
        assert_eq!(cache.get("c").unwrap()[0].text, "丁");
    }

    #[test]
    fn key_ignores_sequence_but_keeps_segmentation() {
        let mut split = request("xian", "xi'an");
        let plain = request("xian", "xian");
        assert_ne!(
            SuggestionCache::key(&split, "zh-CN"),
            SuggestionCache::key(&plain, "zh-CN")
        );
        split.sequence = 99;
        assert_eq!(
            SuggestionCache::key(&split, "zh-CN"),
            SuggestionCache::key(&request("xian", "xi'an"), "zh-CN")
        );
    }

    fn request(letters: &str, pinyin: &str) -> PredictionRequest {
        PredictionRequest {
            sequence: 1,
            kind: qingjian_core::PredictionKind::Compose,
            before: String::new(),
            after: String::new(),
            pinyin: pinyin.to_owned(),
            letters: letters.to_owned(),
            syllables: 0,
            candidates: Vec::new(),
            guess: String::new(),
            max_items: 4,
            want_sentence: false,
            text: String::new(),
            target_language: String::new(),
        }
    }
}

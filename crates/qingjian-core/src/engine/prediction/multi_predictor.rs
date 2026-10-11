//! 组合联想：把多个联想源合到一起，对外仍是一个 [`Predictor`]。
//!
//! 请求同时发给所有源，结果按序号攒成一批：**收齐或超过截止时间**后合并成一份返回。
//! 合并只做拼接与按文本去重，不改各源自己的顺序——在前的源占更靠前的格。
//! 某源 `slots = 0` 时它的词不进候选（只可能带整句），这样「只开整句补全」的配置在组合下依然成立。
//!
//! 截止时间是必需的兜底：某源的线程退出或 panic 时它永远不回结果，只靠「收齐」会一直等下去。

use std::time::{Duration, Instant};

use super::{CloudWord, Prediction, PredictionPolicy, PredictionRequest, Predictor};

/// 一批待合并的结果。
struct Batch {
    /// 这一批对应的请求序号。
    sequence: u64,

    /// 各源的结果，下标与 `MultiPredictor::sources` 对齐，还没到的为 `None`。
    got: Vec<Option<Prediction>>,

    /// 发出这一批的时间，用来算截止。
    started: Instant,
}

/// 把多个联想源当做一个用。
pub struct MultiPredictor {
    /// 各源，顺序就是合并顺序（在前的源占更靠前的槽位）。
    sources: Vec<Box<dyn Predictor>>,

    /// 合并后的策略。
    policy: PredictionPolicy,

    /// 从提交到放弃等待的时限。
    deadline: Duration,

    /// 当前这一批；没有正在等的请求时为 `None`。
    batch: Option<Batch>,
}

impl MultiPredictor {
    /// `deadline` 应取各源超时里最大的那个再加一点余量。
    pub fn new(sources: Vec<Box<dyn Predictor>>, deadline: Duration) -> Self {
        let policy = merge_policy(&sources);
        Self {
            sources,
            policy,
            deadline,
            batch: None,
        }
    }

    /// 把一批结果合成一份：按源顺序拼接词、按文本去重；`slots = 0` 的源只贡献整句。
    fn merge(&self, sequence: u64, got: Vec<Option<Prediction>>) -> Prediction {
        let mut words: Vec<CloudWord> = Vec::new();
        let mut sentence = None;
        for (slot, source) in got.into_iter().zip(&self.sources) {
            let Some(prediction) = slot else {
                continue;
            };
            if source.policy().slots > 0 {
                for word in prediction.words {
                    if !words.iter().any(|kept| kept.text == word.text) {
                        words.push(word);
                    }
                }
            }
            if sentence.is_none() {
                sentence = prediction.sentence;
            }
        }
        Prediction {
            sequence,
            words,
            sentence,
        }
    }
}

/// 合并策略：观察窗口取大、条数上限求和、整句任一源要就给。
fn merge_policy(sources: &[Box<dyn Predictor>]) -> PredictionPolicy {
    let mut policy = PredictionPolicy {
        before: 0,
        after: 0,
        slots: 0,
        max_items: 0,
        sentence: false,
    };
    for source in sources {
        let one = source.policy();
        policy.before = policy.before.max(one.before);
        policy.after = policy.after.max(one.after);
        policy.slots = policy.slots.saturating_add(one.slots);
        policy.max_items = policy.max_items.saturating_add(one.max_items);
        policy.sentence |= one.sentence;
    }
    policy
}

impl Predictor for MultiPredictor {
    fn policy(&self) -> PredictionPolicy {
        self.policy
    }

    fn submit(&mut self, request: PredictionRequest) {
        let sequence = request.sequence;
        for source in &mut self.sources {
            source.submit(request.clone());
        }
        // 新序号覆盖旧批次：上一批还没收齐的结果对应的是上一个输入状态，不要了
        self.batch = Some(Batch {
            sequence,
            got: vec![None; self.sources.len()],
            started: Instant::now(),
        });
    }

    fn poll(&mut self) -> Option<Prediction> {
        // 先抽干所有源，避免通道积压；只收当前序号的
        let sequence = self.batch.as_ref()?.sequence;
        let mut arrived: Vec<(usize, Prediction)> = Vec::new();
        for (index, source) in self.sources.iter_mut().enumerate() {
            while let Some(prediction) = source.poll() {
                arrived.push((index, prediction));
            }
        }
        for (index, prediction) in arrived {
            if prediction.sequence == sequence {
                if let Some(batch) = self.batch.as_mut() {
                    batch.got[index] = Some(prediction);
                }
            } else {
                tracing::debug!(
                    sequence = prediction.sequence,
                    current = sequence,
                    "丢弃过期的联想结果"
                );
            }
        }
        let ready = match &self.batch {
            Some(batch) => {
                batch.got.iter().all(Option::is_some) || batch.started.elapsed() >= self.deadline
            }
            None => return None,
        };
        if !ready {
            return None;
        }
        let batch = self.batch.take()?;
        Some(self.merge(batch.sequence, batch.got))
    }

    fn is_enabled(&self) -> bool {
        !self.sources.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::super::PredictionKind;
    use super::*;

    /// 一个不回网络的假源：提交后按模板回一条结果，或永远不回（模拟失败 / 线程没了）。
    struct Fake {
        policy: PredictionPolicy,
        template: Option<Prediction>,
        queue: VecDeque<Prediction>,
        submitted: Vec<u64>,
    }

    impl Fake {
        fn new(slots: usize, words: &[&str], sentence: Option<&str>) -> Self {
            let template = Prediction {
                sequence: 0,
                words: words.iter().map(|text| word(text)).collect(),
                sentence: sentence.map(str::to_owned),
            };
            Self {
                policy: PredictionPolicy {
                    before: 0,
                    after: 0,
                    slots,
                    max_items: slots + 2,
                    sentence: sentence.is_some(),
                },
                template: Some(template),
                queue: VecDeque::new(),
                submitted: Vec::new(),
            }
        }

        fn silent(slots: usize) -> Self {
            let mut fake = Self::new(slots, &[], None);
            fake.template = None;
            fake
        }
    }

    impl Predictor for Fake {
        fn policy(&self) -> PredictionPolicy {
            self.policy
        }

        fn submit(&mut self, request: PredictionRequest) {
            self.submitted.push(request.sequence);
            if let Some(template) = &self.template {
                self.queue.push_back(Prediction {
                    sequence: request.sequence,
                    words: template.words.clone(),
                    sentence: template.sentence.clone(),
                });
            }
        }

        fn poll(&mut self) -> Option<Prediction> {
            self.queue.pop_front()
        }
    }

    fn word(text: &str) -> CloudWord {
        CloudWord {
            text: text.to_owned(),
            syllables: vec!["a".to_owned()],
            reading: None,
        }
    }

    fn request(sequence: u64) -> PredictionRequest {
        PredictionRequest {
            sequence,
            kind: PredictionKind::Compose,
            before: String::new(),
            after: String::new(),
            pinyin: "yi'si'ta'lu".to_owned(),
            letters: "yisitalu".to_owned(),
            syllables: 4,
            candidates: Vec::new(),
            guess: String::new(),
            max_items: 4,
            want_sentence: false,
            text: String::new(),
            target_language: String::new(),
        }
    }

    fn merge_of(sources: Vec<Box<dyn Predictor>>, request: PredictionRequest) -> Prediction {
        let mut combined = MultiPredictor::new(sources, Duration::from_millis(50));
        combined.submit(request.clone());
        // 假源提交后立刻就有结果，一次 poll 收齐
        combined.poll().expect("收齐后应有结果")
    }

    #[test]
    fn words_are_concatenated_in_source_order_and_deduplicated() {
        let sources: Vec<Box<dyn Predictor>> = vec![
            Box::new(Fake::new(2, &["伊斯塔露", "派蒙"], None)),
            Box::new(Fake::new(2, &["派蒙", "温迪"], None)),
        ];
        let prediction = merge_of(sources, request(1));
        assert_eq!(prediction.sequence, 1);
        let texts: Vec<&str> = prediction.words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, ["伊斯塔露", "派蒙", "温迪"]);
    }

    #[test]
    fn a_source_with_no_slots_contributes_only_its_sentence() {
        let sources: Vec<Box<dyn Predictor>> = vec![
            Box::new(Fake::new(0, &["不要这个词"], Some("整句补全"))),
            Box::new(Fake::new(2, &["伊斯塔露"], None)),
        ];
        let prediction = merge_of(sources, request(1));
        let texts: Vec<&str> = prediction.words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, ["伊斯塔露"]);
        assert_eq!(prediction.sentence.as_deref(), Some("整句补全"));
    }

    #[test]
    fn waits_until_every_source_answers() {
        struct Late {
            policy: PredictionPolicy,
            reply: Option<Prediction>,
            polls: usize,
        }
        impl Predictor for Late {
            fn policy(&self) -> PredictionPolicy {
                self.policy
            }
            fn submit(&mut self, request: PredictionRequest) {
                self.reply = Some(Prediction {
                    sequence: request.sequence,
                    words: vec![word("迟到的词")],
                    sentence: None,
                });
            }
            fn poll(&mut self) -> Option<Prediction> {
                self.polls += 1;
                // 前两次 poll 装作还没好
                if self.polls < 3 {
                    return None;
                }
                self.reply.take()
            }
        }
        let sources: Vec<Box<dyn Predictor>> = vec![
            Box::new(Fake::new(2, &["甲"], None)),
            Box::new(Late {
                policy: PredictionPolicy {
                    before: 0,
                    after: 0,
                    slots: 2,
                    max_items: 4,
                    sentence: false,
                },
                reply: None,
                polls: 0,
            }),
        ];
        let mut combined = MultiPredictor::new(sources, Duration::from_millis(50));
        combined.submit(request(7));
        assert!(combined.poll().is_none(), "慢源还没回，不该提前出结果");
        assert!(combined.poll().is_none(), "慢源还没回，不该提前出结果");
        let prediction = combined.poll().expect("慢源回来后应出结果");
        let texts: Vec<&str> = prediction.words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, ["甲", "迟到的词"]);
    }

    #[test]
    fn a_silent_source_falls_back_to_the_deadline() {
        let sources: Vec<Box<dyn Predictor>> = vec![
            Box::new(Fake::new(2, &["伊斯塔露"], None)),
            Box::new(Fake::silent(2)),
        ];
        let mut combined = MultiPredictor::new(sources, Duration::from_millis(1));
        combined.submit(request(3));
        assert!(combined.poll().is_none(), "还没到截止，继续等");
        std::thread::sleep(Duration::from_millis(5));
        let prediction = combined.poll().expect("超过截止后应给出已到的部分");
        assert_eq!(prediction.words.len(), 1);
        assert_eq!(prediction.words[0].text, "伊斯塔露");
    }

    #[test]
    fn stale_results_are_dropped() {
        // 源只交出一条给旧序号的结果，而组合器在等新序号：这条要丢掉，批次仍没齐
        let mut fake = Fake::silent(2);
        fake.queue.push_back(Prediction {
            sequence: 4,
            words: vec![word("旧")],
            sentence: None,
        });
        let sources: Vec<Box<dyn Predictor>> = vec![Box::new(fake)];
        let mut combined = MultiPredictor::new(sources, Duration::from_millis(30));
        combined.submit(request(5));
        assert!(combined.poll().is_none(), "旧序号的结果要丢掉，批次还没齐");
    }

    #[test]
    fn a_batch_emits_once_then_stops() {
        let sources: Vec<Box<dyn Predictor>> = vec![Box::new(Fake::new(2, &["伊斯塔露"], None))];
        let mut combined = MultiPredictor::new(sources, Duration::from_millis(30));
        combined.submit(request(2));
        let prediction = combined.poll().expect("收齐后应有结果");
        assert_eq!(prediction.sequence, 2);
        // 一个序号只发一次
        assert!(combined.poll().is_none());
    }

    #[test]
    fn policy_is_summed_and_any_sentence_wins() {
        let sources: Vec<Box<dyn Predictor>> = vec![
            Box::new(Fake::new(2, &[], None)),
            Box::new(Fake::new(3, &[], Some("句子"))),
        ];
        let combined = MultiPredictor::new(sources, Duration::from_millis(50));
        let policy = combined.policy();
        assert_eq!(policy.slots, 5);
        assert!(policy.sentence);
        assert!(combined.is_enabled());
    }

    #[test]
    fn no_sources_means_not_enabled() {
        let combined = MultiPredictor::new(Vec::new(), Duration::from_millis(50));
        assert!(!combined.is_enabled());
    }
}

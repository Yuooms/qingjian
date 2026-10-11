//! 后台线程：收请求、防抖、查缓存、发查询、回结果。

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use qingjian_core::parser;
use qingjian_core::{CloudWord, Prediction, PredictionKind, PredictionRequest};

use crate::cache::SuggestionCache;
use crate::client::SuggestClient;
use crate::config::SearchConfig;
use crate::error::SearchError;
use crate::filter::to_cloud_words;

/// 缓存条数。
const CACHE_CAPACITY: usize = 64;

pub struct Worker {
    /// 请求入口。主线程 drop 掉发送端后线程自然退出。
    requests: Receiver<PredictionRequest>,

    /// 结果出口。
    responses: Sender<Prediction>,

    /// 网络客户端。
    client: SuggestClient,

    /// 防抖窗口。
    debounce: Duration,

    /// 一次最多取几条（比槽位多两条，去重后还填得满）；`[search] slots = 0` 时为 0，表示不要词。
    max_items: usize,

    /// 少于这么多个字母不发请求。
    min_letters: usize,

    /// 结果缓存。
    cache: SuggestionCache,
}

impl Worker {
    pub fn new(
        requests: Receiver<PredictionRequest>,
        responses: Sender<Prediction>,
        config: &SearchConfig,
    ) -> Self {
        Self {
            requests,
            responses,
            client: SuggestClient::new(config),
            debounce: Duration::from_millis(config.debounce_ms),
            max_items: if config.slots == 0 {
                0
            } else {
                config.slots + 2
            },
            min_letters: config.min_letters,
            cache: SuggestionCache::with_capacity(CACHE_CAPACITY),
        }
    }

    /// 阻塞运行直到发送端全部关闭。
    pub fn run(mut self) -> Result<(), SearchError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        while let Ok(first) = self.requests.recv() {
            let Some(request) = self.debounce(first) else {
                return Ok(());
            };
            // 不发的请求也回一条空结果：组合联想靠它把这一批收齐，早点结束等待
            if !self.wants(&request) {
                self.reply(request.sequence, Vec::new());
                continue;
            }
            let key = SuggestionCache::key(&request, self.client.market());
            if let Some(words) = self.cache.get(&key) {
                tracing::debug!(sequence = request.sequence, "搜索联想命中缓存");
                self.reply(request.sequence, words.clone());
                continue;
            }
            let start = Instant::now();
            match runtime.block_on(self.client.suggest(&request.letters)) {
                Ok(suggestions) => {
                    let words = to_cloud_words(suggestions, &request.pinyin, self.max_items);
                    tracing::info!(
                        sequence = request.sequence,
                        elapsed_ms = start.elapsed().as_millis(),
                        words = words.len(),
                        "搜索联想完成"
                    );
                    // 空结果不进缓存：接口偶尔什么都不给，缓存住就等于这段拼音以后永远没联想
                    if !words.is_empty() {
                        self.cache.insert(key, words.clone());
                    }
                    self.reply(request.sequence, words);
                }
                Err(error) => {
                    tracing::warn!(sequence = request.sequence, %error, "搜索联想失败");
                    // 失败回空：不阻塞输入，也让组合联想尽快收齐
                    self.reply(request.sequence, Vec::new());
                }
            }
        }
        Ok(())
    }

    /// 这次请求要不要发：只在组句中、不是简拼、字母够多、而且能切成完整拼音。
    /// 简拼（`fhyq`）搜索接口只会给按声母凑出来的短语；`request.max_items` 被 Core 置 0 正是简拼的标记。
    /// 另外 `[search] slots = 0` 表示不要词，那一次也别问（自己的 `max_items` 为 0）。
    fn wants(&self, request: &PredictionRequest) -> bool {
        self.max_items > 0
            && request.kind == PredictionKind::Compose
            && request.max_items > 0
            && request.letters.len() >= self.min_letters
            && parser::is_fully_segmentable(&request.letters)
    }

    /// 防抖：窗口内持续收到新请求就一直等，只保留最后一个。发送端关闭返回 `None`。
    fn debounce(&self, mut latest: PredictionRequest) -> Option<PredictionRequest> {
        loop {
            match self.requests.recv_timeout(self.debounce) {
                Ok(newer) => latest = newer,
                Err(RecvTimeoutError::Timeout) => return Some(latest),
                Err(RecvTimeoutError::Disconnected) => return None,
            }
        }
    }

    fn reply(&self, sequence: u64, words: Vec<CloudWord>) {
        // 接收端没了说明 Predictor 已经被 drop，线程随后也会退出
        let _ = self.responses.send(Prediction {
            sequence,
            words,
            sentence: None,
        });
    }
}

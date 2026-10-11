//! 搜索联想连通性测试：设置「搜索联想」页的「测试联通」按钮用。
//! 起一个线程用当前配置查一次固定拼音，结果经通道回来；调用方在主线程轮询 [`SearchProbe::poll`]。

use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::client::SuggestClient;
use crate::config::SearchConfig;
use crate::error::SearchError;

/// 探活用的拼音：常用词，哪个市场下都该有建议。
const PROBE_LETTERS: &str = "zhongguo";

/// 回给界面的建议条数上限：够看出接口通没通就行，不必把整页搬进设置窗口。
const MAX_SAMPLES: usize = 5;

/// 连通性测试成功时的结果。
pub struct SearchReport {
    /// 探活用的拼音。
    pub letters: &'static str,

    /// 接口返回的建议条数。
    pub count: usize,

    /// 前几条建议，给界面预览。
    pub samples: Vec<String>,

    /// 从发出到收到回复的时间。
    pub elapsed: Duration,
}

/// 一次进行中的连通性测试。
pub struct SearchProbe {
    /// 测试线程送回的结果；线程只发一次。
    result: mpsc::Receiver<Result<SearchReport, SearchError>>,
}

impl SearchProbe {
    /// 按配置发起测试。线程起不来当场报错，其余错误从 [`poll`](Self::poll) 回来。
    pub fn start(config: &SearchConfig) -> Result<Self, SearchError> {
        let client = SuggestClient::new(config);
        let (sender, result) = mpsc::channel();
        std::thread::Builder::new()
            .name("qingjian-search-probe".to_owned())
            .spawn(move || {
                let outcome = run(&client);
                match &outcome {
                    Ok(report) => tracing::info!(
                        letters = report.letters,
                        count = report.count,
                        elapsed_ms = report.elapsed.as_millis() as u64,
                        "搜索联想连通性测试成功"
                    ),
                    Err(error) => tracing::warn!(error = ?error, "搜索联想连通性测试失败"),
                }
                // 收的一方不在了（窗口关了、又点了一次）就算了
                let _ = sender.send(outcome);
            })?;
        Ok(Self { result })
    }

    /// 结果到了就取走；没到返回 `None`。取走之后再调永远是 `None`。
    pub fn poll(&self) -> Option<Result<SearchReport, SearchError>> {
        self.result.try_recv().ok()
    }
}

/// 在测试线程里跑：建一个单线程运行时，查一次固定拼音，计时。
fn run(client: &SuggestClient) -> Result<SearchReport, SearchError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let started = Instant::now();
    let suggestions = runtime.block_on(client.suggest(PROBE_LETTERS))?;
    Ok(SearchReport {
        letters: PROBE_LETTERS,
        count: suggestions.len(),
        samples: suggestions.into_iter().take(MAX_SAMPLES).collect(),
        elapsed: started.elapsed(),
    })
}

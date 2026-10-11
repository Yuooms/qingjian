use std::sync::mpsc::{self, Receiver, Sender};

use qingjian_core::{Prediction, PredictionPolicy, PredictionRequest, Predictor};

use crate::config::SearchConfig;
use crate::error::SearchError;
use crate::worker::Worker;

/// 走搜索建议接口的联想器。构造时起一个后台线程，`submit` / `poll` 都只碰通道，不阻塞。
pub struct BingPredictor {
    /// 观察窗口、条数上限与开关，来自配置。
    policy: PredictionPolicy,

    /// 往后台线程发请求。
    requests: Sender<PredictionRequest>,

    /// 从后台线程收结果。
    responses: Receiver<Prediction>,
}

impl BingPredictor {
    pub fn new(config: &SearchConfig) -> Result<Self, SearchError> {
        let (requests, request_rx) = mpsc::channel();
        let (response_tx, responses) = mpsc::channel();
        let worker = Worker::new(request_rx, response_tx, config);
        std::thread::Builder::new()
            .name("qingjian-search".to_owned())
            .spawn(move || {
                if let Err(error) = worker.run() {
                    tracing::error!(%error, "搜索联想线程退出");
                }
            })?;
        tracing::info!(
            endpoint = %config.endpoint,
            market = %config.market,
            slots = config.slots,
            "搜索联想已启用"
        );
        Ok(Self {
            policy: config.policy(),
            requests,
            responses,
        })
    }
}

impl Predictor for BingPredictor {
    fn policy(&self) -> PredictionPolicy {
        self.policy
    }

    fn submit(&mut self, request: PredictionRequest) {
        if self.requests.send(request).is_err() {
            tracing::warn!("搜索联想线程已退出，请求被丢弃");
        }
    }

    fn poll(&mut self) -> Option<Prediction> {
        // Empty 与 Disconnected 都当没有：线程退出时已经记过日志
        self.responses.try_recv().ok()
    }
}

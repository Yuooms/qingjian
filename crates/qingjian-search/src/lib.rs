//! 搜索联想：[`qingjian_core::Predictor`] 的第二个网络实现，接搜索建议接口（缺省 Bing）。
//!
//! 用户输入拼音时，把拼音字母发给搜索建议接口，取回的中文联想词补进候选列表——词库中没有的词
//! （人名、地名、新词）也能打出来。请求在独立线程里发，主线程只往通道里丢请求、从通道里取结果，
//! 永远不阻塞输入；失败只记日志、回一条空结果。
//!
//! 与云联想（`qingjian-predict`）可以同时开启，由 Core 的 [`qingjian_core::MultiPredictor`] 合并两路结果。
//!
//! 设置程序的「搜索联想」页用 [`SearchProbe`] 发一条固定查询做连通性测试。

mod bing_predictor;
mod cache;
mod client;
mod config;
mod error;
mod filter;
mod parse;
mod probe;
mod worker;

pub use bing_predictor::BingPredictor;
pub use config::SearchConfig;
pub use error::SearchError;
pub use probe::{SearchProbe, SearchReport};

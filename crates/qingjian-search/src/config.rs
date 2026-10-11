use qingjian_core::PredictionPolicy;
use serde::{Deserialize, Serialize};

/// 搜索联想配置。默认**关闭**，开启后用户敲的拼音字母会发往 `endpoint`。
///
/// 与云联想（`[predict]`）是两个独立开关，可以同时开启：两路结果一起补进候选末尾。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    /// 是否启用。
    pub enabled: bool,

    /// 搜索建议接口地址（OpenSearch Suggestions JSON）。
    pub endpoint: String,

    /// 市场（`mkt`），决定语言与地区；简体中文用 `zh-CN`。
    pub market: String,

    /// 界面语言（`setlang`）。
    pub setlang: String,

    /// 单次请求超时（毫秒），超时即丢。
    pub timeout_ms: u64,

    /// 防抖：停止敲键多久之后才真正发请求（毫秒）。
    pub debounce_ms: u64,

    /// 联想词最多补进候选窗口第一页末尾几格。0 = 不要词（既不占格也不发请求，等同关掉）。
    pub slots: usize,

    /// 少于这么多个字母不发请求（单字母查询没有意义，也省一次往返）。
    pub min_letters: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "https://api.bing.com/osjson.aspx".to_owned(),
            market: "zh-CN".to_owned(),
            setlang: "zh-Hans".to_owned(),
            timeout_ms: 1500,
            debounce_ms: 200,
            slots: 2,
            min_letters: 2,
        }
    }
}

impl SearchConfig {
    /// 只管拼音，不看上下文：观察窗口为 0，请求里不带光标附近的文字。
    pub fn policy(&self) -> PredictionPolicy {
        PredictionPolicy {
            before: 0,
            after: 0,
            slots: self.slots,
            // 比槽位多要几条，与本地候选重复的去掉后还能填满；不要词（0 格）就一条也别要，
            // `max_items = 0` 是引擎里「这个来源不用问」的信号，能省掉一次往返。
            max_items: if self.slots == 0 { 0 } else { self.slots + 2 },
            sentence: false,
        }
    }
}

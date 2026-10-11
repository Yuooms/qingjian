//! 「测试连接 / 测试联通」这类按钮的状态，挂在 [`Settings`](super::Settings) 上，
//! 由根组件的 `update` 推进、对应的页显示。云服务页与搜索联想页共用。

/// 一次连通性测试的状态。
#[derive(Clone)]
pub(crate) enum ProbeStatus {
    /// 没测过。
    Idle,

    /// 正在测。
    Testing,

    /// 成功（带说明）。
    Ok(String),

    /// 失败（带错误）。
    Failed(String),
}

//! 错误类型定义

use thiserror::Error;

/// MT4 客户端错误类型
#[derive(Error, Debug)]
pub enum Mt4Error {
    /// HTTP 请求错误
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// WebSocket 错误
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    /// 加密错误
    #[error("Encryption error: {0}")]
    Encryption(String),

    /// 解密错误
    #[error("Decryption error: {0}")]
    Decryption(String),

    /// 认证失败
    #[error("Authentication failed: code {0}")]
    AuthFailed(u8),

    /// 交易错误
    #[error("Trade error: {message} (code: {code})")]
    Trade { code: u8, message: String },

    /// 连接错误
    #[error("Connection error: {0}")]
    Connection(String),

    /// 协议错误
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// 未连接
    #[error("Not connected")]
    NotConnected,

    /// 超时
    #[error("Operation timeout")]
    Timeout,

    /// 服务器错误
    #[error("Server error: {0}")]
    Server(String),

    /// 无效参数
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
}

/// 交易错误码映射
impl Mt4Error {
    /// 从交易错误码创建错误
    pub fn from_trade_code(code: u8) -> Self {
        let message = match code {
            0 => "Success",
            1 => "Request sent",
            2 => "Common error",
            3 => "Invalid parameters",
            4 => "Server busy",
            5 => "Old version",
            6 => "No connection",
            7 => "Not enough rights",
            8 => "Too frequent requests",
            64 => "Account disabled",
            65 => "Invalid account",
            66 => "Public key not found",
            128 => "Trade timeout",
            129 => "Invalid prices",
            130 => "Invalid S/L or T/P",
            131 => "Invalid volume",
            132 => "Market is closed",
            133 => "Trade is disabled",
            134 => "Not enough money",
            135 => "Price is changed",
            136 => "Off quotes",
            137 => "Broker is busy",
            138 => "Requote",
            139 => "Order is locked",
            140 => "Only long positions allowed",
            141 => "Too many requests",
            142 => "Order accepted",
            143 => "Order in process",
            144 => "Request canceled",
            145 => "Modification denied",
            146 => "Trade context busy",
            147 => "Expiration denied",
            148 => "Too many orders",
            149 => "Hedge prohibited",
            150 => "FIFO rule violated",
            _ => "Unknown error",
        };
        Mt4Error::Trade {
            code,
            message: message.to_string(),
        }
    }
}

/// 结果类型别名
pub type Result<T> = std::result::Result<T, Mt4Error>;

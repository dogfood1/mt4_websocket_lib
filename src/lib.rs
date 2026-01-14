//! MT4 WebSocket Client Library
//!
//! 用于连接 MetaTrader 4 Web Terminal 的 Rust 客户端库
//!
//! # 功能
//! - HTTP API 获取认证 token
//! - WebSocket 连接和通信
//! - AES-256-CBC 加密/解密
//! - 交易操作（下单、平仓、修改订单）
//! - 实时报价和订单更新
//!
//! # 示例
//! ```no_run
//! use mt4_client::{Mt4Client, LoginCredentials};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let credentials = LoginCredentials {
//!         login: "31313724".to_string(),
//!         password: "password".to_string(),
//!         server: "ICMarketsSC-Demo03".to_string(),
//!     };
//!
//!     let mut client = Mt4Client::new();
//!     client.connect(&credentials).await?;
//!
//!     // 下单
//!     client.buy("EURUSD", 0.01, None, None).await?;
//!
//!     Ok(())
//! }
//! ```

pub mod api;
pub mod client;
pub mod crypto;
pub mod error;
pub mod protocol;
pub mod types;

pub use api::Mt4Api;
pub use client::{Mt4Client, Mt4Event, PendingRequest, RequestTracker};
pub use error::{Mt4Error, Result};
pub use protocol::{Command, OrderType, TradeType};
pub use types::*;

/// 登录凭证
#[derive(Debug, Clone)]
pub struct LoginCredentials {
    pub login: String,
    pub password: String,
    pub server: String,
}

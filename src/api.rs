//! HTTP API 模块 - 获取认证 token

use crate::error::{Mt4Error, Result};
use serde::{Deserialize, Serialize};

/// MT4 Web API 基础 URL
const BASE_URL: &str = "https://metatraderweb.app";

/// Token 响应
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    /// 信号服务器地址
    pub signal_server: String,
    /// 交易服务器
    pub trade_server: String,
    /// 登录账号
    pub login: String,
    /// 经纪商公司名
    pub company: Option<String>,
    /// Ping 值
    pub ping: Option<i32>,
    /// 会话密钥 (64位十六进制)
    pub key: String,
    /// 认证 token
    pub token: String,
    /// 协议版本
    pub version: Option<i32>,
    /// 是否启用
    pub enabled: bool,
    /// 可用的网关服务器列表
    pub gwt_servers: Option<Vec<i32>>,
    /// SSL 是否启用
    pub ssl: Option<bool>,
    /// 错误信息
    pub error: Option<String>,
}

/// Token 请求参数
#[derive(Debug, Serialize)]
struct TokenRequest {
    login: String,
    trade_server: String,
    gwt: i32,
}

/// MT4 HTTP API 客户端
pub struct Mt4Api {
    client: reqwest::Client,
    base_url: String,
}

impl Mt4Api {
    /// 创建新的 API 客户端
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: BASE_URL.to_string(),
        }
    }

    /// 使用自定义基础 URL 创建 API 客户端
    pub fn with_base_url(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
        }
    }

    /// 获取认证 token
    ///
    /// # 参数
    /// - `login`: MT4 账号
    /// - `server`: 交易服务器名称 (如 "ICMarketsSC-Demo03")
    /// - `gwt`: 网关编号 (1-8)
    ///
    /// # 返回
    /// - `TokenResponse`: 包含 token, key, signal_server 等信息
    ///
    /// # 注意
    /// 此请求不包含密码！密码在 WebSocket 认证阶段发送
    pub async fn get_token(&self, login: &str, server: &str, gwt: i32) -> Result<TokenResponse> {
        let url = format!("{}/trade/json", self.base_url);

        let params = [
            ("login", login),
            ("trade_server", server),
            ("gwt", &gwt.to_string()),
        ];

        tracing::debug!("Requesting token for login: {}, server: {}", login, server);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "*/*")
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(Mt4Error::Server(format!(
                "HTTP {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let token_response: TokenResponse = response.json().await?;

        if let Some(error) = &token_response.error {
            return Err(Mt4Error::Server(error.clone()));
        }

        if !token_response.enabled {
            return Err(Mt4Error::Server("Web Terminal not supported".to_string()));
        }

        tracing::info!(
            "Token received, signal_server: {}",
            token_response.signal_server
        );

        Ok(token_response)
    }

    /// 获取服务器列表
    pub async fn get_servers(&self, broker: &str) -> Result<serde_json::Value> {
        let url = format!("{}/trade/servers/{}", self.base_url, broker);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(Mt4Error::Server(format!(
                "HTTP {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let data: serde_json::Value = response.json().await?;
        Ok(data)
    }
}

impl Default for Mt4Api {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要网络连接
    async fn test_get_token() {
        let api = Mt4Api::new();
        let result = api.get_token("31313724", "ICMarketsSC-Demo03", 4).await;
        assert!(result.is_ok());
        let token = result.unwrap();
        assert!(!token.token.is_empty());
        assert!(!token.key.is_empty());
    }
}

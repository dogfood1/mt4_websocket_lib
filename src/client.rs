//! MT4 WebSocket 客户端

use crate::api::{Mt4Api, TokenResponse};
use crate::crypto::Mt4Crypto;
use crate::error::{Mt4Error, Result};
use crate::protocol::{Command, AUTH_DATA_SIZE};
use crate::types::{OrderUpdate, TradeRequest};
use crate::LoginCredentials;
use byteorder::{LittleEndian, WriteBytesExt};
use futures_util::{SinkExt, StreamExt};
use std::io::Cursor;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// 客户端事件
#[derive(Debug, Clone)]
pub enum Mt4Event {
    /// 连接成功
    Connected,
    /// 认证成功
    Authenticated,
    /// 认证失败
    AuthFailed(u8),
    /// 订单更新
    OrderUpdate(OrderUpdate),
    /// 交易成功
    TradeSuccess { request_id: i32, status: i32 },
    /// 交易失败
    TradeFailed { code: u8, message: String },
    /// 连接断开
    Disconnected,
    /// 错误
    Error(String),
    /// Pong 响应
    Pong,
    /// 原始消息 (未识别的命令)
    RawMessage { command: u16, error_code: u8, data: Vec<u8> },
}

/// MT4 WebSocket 客户端
pub struct Mt4Client {
    /// API 客户端
    api: Mt4Api,
    /// 加密器
    crypto: Arc<Mutex<Mt4Crypto>>,
    /// WebSocket 写端
    writer: Option<mpsc::Sender<Vec<u8>>>,
    /// 事件接收器
    event_rx: Option<mpsc::Receiver<Mt4Event>>,
    /// 是否已认证
    authenticated: bool,
    /// Token 信息
    token_info: Option<TokenResponse>,
}

impl Mt4Client {
    /// 创建新的客户端
    pub fn new() -> Self {
        Self {
            api: Mt4Api::new(),
            crypto: Arc::new(Mutex::new(Mt4Crypto::default())),
            writer: None,
            event_rx: None,
            authenticated: false,
            token_info: None,
        }
    }

    /// 连接到 MT4 服务器
    pub async fn connect(&mut self, credentials: &LoginCredentials) -> Result<()> {
        tracing::info!(
            "Connecting to MT4: login={}, server={}",
            credentials.login,
            credentials.server
        );

        // 1. 获取 token
        let token_info = self.api.get_token(&credentials.login, &credentials.server, 4).await?;
        tracing::info!("Token received: {}", &token_info.token[..20.min(token_info.token.len())]);

        // 2. 设置会话密钥
        {
            let mut crypto = self.crypto.lock().await;
            crypto.set_session_key(&token_info.key)?;
            tracing::debug!("Session key set: {}", &token_info.key[..20.min(token_info.key.len())]);
        }

        // 3. 构建 WebSocket URL
        let use_ssl = token_info.ssl.unwrap_or(true);
        let protocol = if use_ssl { "wss" } else { "ws" };
        let mut signal_server = token_info.signal_server.clone();
        if signal_server.ends_with(":443") {
            signal_server = signal_server.replace(":443", "");
        }
        let ws_url = format!("{}://{}/", protocol, signal_server);
        tracing::info!("Connecting to WebSocket: {}", ws_url);

        // 4. 连接 WebSocket
        let (ws_stream, _) = connect_async(&ws_url).await?;
        let (write, read) = ws_stream.split();

        // 5. 创建通道
        let (write_tx, mut write_rx) = mpsc::channel::<Vec<u8>>(32);
        let (event_tx, event_rx) = mpsc::channel::<Mt4Event>(64);

        self.writer = Some(write_tx.clone());
        self.event_rx = Some(event_rx);
        self.token_info = Some(token_info.clone());

        // 6. 启动写入任务
        let write = Arc::new(Mutex::new(write));
        let write_clone = write.clone();
        tokio::spawn(async move {
            while let Some(data) = write_rx.recv().await {
                let mut w = write_clone.lock().await;
                if let Err(e) = w.send(Message::Binary(data)).await {
                    tracing::error!("WebSocket write error: {}", e);
                    break;
                }
            }
        });

        // 7. 启动读取任务
        let crypto = self.crypto.clone();
        let password = credentials.password.clone();
        let token = token_info.token.clone();
        let write_tx_clone = write_tx.clone();

        tokio::spawn(async move {
            let mut read = read;
            let mut pending_auth = true;
            let mut password_sent = false;

            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Binary(data)) => {
                        // 解密消息
                        let crypto_guard = crypto.lock().await;
                        if data.len() < 8 {
                            continue;
                        }

                        let payload = &data[8..];
                        let decrypted = match crypto_guard.decrypt(payload) {
                            Ok(d) => d,
                            Err(e) => {
                                tracing::error!("Decrypt error: {}", e);
                                continue;
                            }
                        };
                        drop(crypto_guard);

                        if decrypted.len() < 5 {
                            continue;
                        }

                        let command = u16::from_le_bytes([decrypted[2], decrypted[3]]);
                        let error_code = decrypted[4];
                        let msg_data = decrypted[5..].to_vec();

                        tracing::debug!(
                            "Received: command={}, error={}, data_len={}",
                            command,
                            error_code,
                            msg_data.len()
                        );

                        // 处理消息
                        match command {
                            0 if pending_auth && !password_sent => {
                                // Token 确认，发送密码
                                tracing::info!("Token accepted, sending password...");
                                let pwd_data = Self::encode_password(&password);
                                let crypto_guard = crypto.lock().await;
                                if let Ok(packet) = Self::build_packet(
                                    Command::AuthPassword as u16,
                                    &pwd_data,
                                    &crypto_guard,
                                    false,
                                ) {
                                    drop(crypto_guard);
                                    let _ = write_tx_clone.send(packet).await;
                                    password_sent = true;
                                }
                            }
                            1 => {
                                // 认证响应
                                if error_code == 0 {
                                    pending_auth = false;
                                    tracing::info!("Authentication successful!");
                                    let _ = event_tx.send(Mt4Event::Authenticated).await;
                                } else {
                                    tracing::error!("Authentication failed: {}", error_code);
                                    let _ = event_tx.send(Mt4Event::AuthFailed(error_code)).await;
                                }
                            }
                            10 => {
                                // 订单更新
                                if let Some(update) = OrderUpdate::from_bytes(&msg_data) {
                                    tracing::info!(
                                        "Order update: ticket={}, symbol={}, type={:?}",
                                        update.order.ticket,
                                        update.order.symbol,
                                        update.order.order_type
                                    );
                                    let _ = event_tx.send(Mt4Event::OrderUpdate(update)).await;
                                }
                            }
                            12 => {
                                // 交易响应
                                let request_id = if msg_data.len() >= 4 {
                                    i32::from_le_bytes([msg_data[0], msg_data[1], msg_data[2], msg_data[3]])
                                } else {
                                    0
                                };
                                let status = if msg_data.len() >= 8 {
                                    i32::from_le_bytes([msg_data[4], msg_data[5], msg_data[6], msg_data[7]])
                                } else {
                                    0
                                };

                                // 检查 error_code 或 status 是否有错误
                                if error_code != 0 {
                                    let err = Mt4Error::from_trade_code(error_code);
                                    if let Mt4Error::Trade { code, message } = err {
                                        tracing::warn!("Trade failed (error_code): code={}, msg={}", code, message);
                                        let _ = event_tx.send(Mt4Event::TradeFailed { code, message }).await;
                                    }
                                } else if status != 0 {
                                    // status 非0也是错误
                                    let err = Mt4Error::from_trade_code(status as u8);
                                    if let Mt4Error::Trade { code, message } = err {
                                        tracing::warn!("Trade failed (status): code={}, msg={}", code, message);
                                        let _ = event_tx.send(Mt4Event::TradeFailed { code, message }).await;
                                    }
                                } else {
                                    tracing::info!("Trade success: request_id={}", request_id);
                                    let _ = event_tx.send(Mt4Event::TradeSuccess { request_id, status }).await;
                                }
                            }
                            51 => {
                                // Pong
                                tracing::trace!("Pong received");
                                let _ = event_tx.send(Mt4Event::Pong).await;
                            }
                            _ => {
                                let _ = event_tx.send(Mt4Event::RawMessage {
                                    command,
                                    error_code,
                                    data: msg_data,
                                }).await;
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        tracing::info!("WebSocket closed");
                        let _ = event_tx.send(Mt4Event::Disconnected).await;
                        break;
                    }
                    Err(e) => {
                        tracing::error!("WebSocket error: {}", e);
                        let _ = event_tx.send(Mt4Event::Error(e.to_string())).await;
                        break;
                    }
                    _ => {}
                }
            }
        });

        // 8. 发送 token
        let token_data = Self::encode_token(&token);
        let crypto_guard = self.crypto.lock().await;
        let packet = Self::build_packet(Command::AuthToken as u16, &token_data, &crypto_guard, true)?;
        drop(crypto_guard);

        if let Some(writer) = &self.writer {
            writer.send(packet).await.map_err(|_| Mt4Error::Connection("Send failed".to_string()))?;
        }

        Ok(())
    }

    /// 编码 token (64字节 ASCII)
    fn encode_token(token: &str) -> Vec<u8> {
        let mut buffer = vec![0u8; AUTH_DATA_SIZE];
        let bytes = token.as_bytes();
        let len = bytes.len().min(AUTH_DATA_SIZE);
        buffer[..len].copy_from_slice(&bytes[..len]);
        buffer
    }

    /// 编码密码 (64字节 UTF-16 LE)
    fn encode_password(password: &str) -> Vec<u8> {
        let mut buffer = vec![0u8; AUTH_DATA_SIZE];
        for (i, c) in password.chars().take(32).enumerate() {
            let code = c as u16;
            buffer[i * 2] = (code & 0xFF) as u8;
            buffer[i * 2 + 1] = (code >> 8) as u8;
        }
        buffer
    }

    /// 构建数据包
    fn build_packet(
        command: u16,
        data: &[u8],
        crypto: &Mt4Crypto,
        use_auth_key: bool,
    ) -> Result<Vec<u8>> {
        // 4字节头 + 数据
        let mut payload = vec![0u8; 4 + data.len()];
        payload[0] = rand::random();
        payload[1] = rand::random();
        payload[2] = (command & 0xFF) as u8;
        payload[3] = (command >> 8) as u8;
        payload[4..].copy_from_slice(data);

        // 加密
        let encrypted = crypto.encrypt(&payload, use_auth_key)?;

        // 8字节头 + 加密数据
        let mut packet = vec![0u8; 8 + encrypted.len()];
        let mut cursor = Cursor::new(&mut packet[..]);
        cursor.write_u32::<LittleEndian>(encrypted.len() as u32).unwrap();
        cursor.write_u32::<LittleEndian>(1).unwrap();
        packet[8..].copy_from_slice(&encrypted);

        Ok(packet)
    }

    /// 发送命令
    pub async fn send_command(&self, command: Command, data: &[u8]) -> Result<()> {
        let crypto = self.crypto.lock().await;
        let packet = Self::build_packet(command as u16, data, &crypto, false)?;
        drop(crypto);

        if let Some(writer) = &self.writer {
            writer
                .send(packet)
                .await
                .map_err(|_| Mt4Error::Connection("Send failed".to_string()))?;
        } else {
            return Err(Mt4Error::NotConnected);
        }

        Ok(())
    }

    /// 发送交易请求
    pub async fn send_trade(&self, request: TradeRequest) -> Result<()> {
        tracing::info!(
            "Sending trade: {:?} {} {} lots @ {}",
            request.order_type,
            request.symbol,
            request.volume,
            request.price
        );
        let data = request.to_bytes();
        self.send_command(Command::TradeRequest, &data).await
    }

    /// 市价买入
    pub async fn buy(&self, symbol: &str, volume: f64, sl: Option<f64>, tp: Option<f64>) -> Result<()> {
        let request = TradeRequest::buy(symbol, volume, sl.unwrap_or(0.0), tp.unwrap_or(0.0));
        self.send_trade(request).await
    }

    /// 市价卖出
    pub async fn sell(&self, symbol: &str, volume: f64, sl: Option<f64>, tp: Option<f64>) -> Result<()> {
        let request = TradeRequest::sell(symbol, volume, sl.unwrap_or(0.0), tp.unwrap_or(0.0));
        self.send_trade(request).await
    }

    /// 限价买入
    pub async fn buy_limit(
        &self,
        symbol: &str,
        volume: f64,
        price: f64,
        sl: Option<f64>,
        tp: Option<f64>,
    ) -> Result<()> {
        let request = TradeRequest::buy_limit(symbol, volume, price, sl.unwrap_or(0.0), tp.unwrap_or(0.0));
        self.send_trade(request).await
    }

    /// 限价卖出
    pub async fn sell_limit(
        &self,
        symbol: &str,
        volume: f64,
        price: f64,
        sl: Option<f64>,
        tp: Option<f64>,
    ) -> Result<()> {
        let request = TradeRequest::sell_limit(symbol, volume, price, sl.unwrap_or(0.0), tp.unwrap_or(0.0));
        self.send_trade(request).await
    }

    /// 平仓
    pub async fn close_order(&self, ticket: i32, symbol: &str, volume: f64) -> Result<()> {
        let request = TradeRequest::close(ticket, symbol, volume);
        self.send_trade(request).await
    }

    /// 发送 Ping
    pub async fn ping(&self) -> Result<()> {
        self.send_command(Command::Ping, &[]).await
    }

    /// 请求账户信息
    pub async fn request_account_info(&self) -> Result<()> {
        self.send_command(Command::AccountInfo, &[]).await
    }

    /// 请求订单列表
    pub async fn request_orders(&self) -> Result<()> {
        self.send_command(Command::OrdersRequest, &[]).await
    }

    /// 接收下一个事件
    pub async fn next_event(&mut self) -> Option<Mt4Event> {
        if let Some(rx) = &mut self.event_rx {
            rx.recv().await
        } else {
            None
        }
    }

    /// 是否已连接
    pub fn is_connected(&self) -> bool {
        self.writer.is_some()
    }

    /// 断开连接
    pub async fn disconnect(&mut self) {
        self.writer = None;
        self.event_rx = None;
        self.authenticated = false;
    }
}

impl Default for Mt4Client {
    fn default() -> Self {
        Self::new()
    }
}

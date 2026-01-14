//! MT4 WebSocket 客户端

use crate::api::{Mt4Api, TokenResponse};
use crate::crypto::Mt4Crypto;
use crate::error::{Mt4Error, Result};
use crate::protocol::{Command, AUTH_DATA_SIZE};
use crate::types::{AccountInfo, Order, OrderUpdate, TradeRequest};
use crate::LoginCredentials;
use byteorder::{LittleEndian, WriteBytesExt};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// 待确认的交易请求
/// 根据 JS mt4.en.js 第1183行: N[b.kj] = b (待确认请求映射)
#[derive(Debug, Clone)]
pub struct PendingRequest {
    /// 请求ID
    pub request_id: i32,
    /// 原始请求
    pub request: TradeRequest,
    /// 创建时间
    pub created_at: Instant,
    /// 目标ticket (平仓/取消/修改操作时有值)
    pub target_ticket: Option<i32>,
}

/// 请求追踪器
/// 根据 JS mt4.en.js 第1216行初始化:
/// - N = {}  待确认请求
/// - W = {}  超时定时器
/// - E = {}  ticket防重复
/// - B.GH = 1000  request_id计数器
#[derive(Debug)]
pub struct RequestTracker {
    /// request_id 计数器 (从1000开始，与JS一致)
    next_request_id: AtomicI32,
    /// 待确认请求: request_id -> PendingRequest
    /// 对应 JS 的 N[]
    pending_requests: RwLock<HashMap<i32, PendingRequest>>,
    /// ticket 防重复: ticket -> request_id
    /// 对应 JS 的 E[]
    /// 防止同一个ticket同时有多个操作
    ticket_locks: RwLock<HashMap<i32, i32>>,
}

impl Default for RequestTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestTracker {
    /// 创建新的请求追踪器
    pub fn new() -> Self {
        Self {
            // 根据 JS: B.GH = 1000
            next_request_id: AtomicI32::new(1000),
            pending_requests: RwLock::new(HashMap::new()),
            ticket_locks: RwLock::new(HashMap::new()),
        }
    }

    /// 生成下一个 request_id
    /// 对应 JS: b.kj = B.GH++
    pub fn next_id(&self) -> i32 {
        self.next_request_id.fetch_add(1, Ordering::SeqCst)
    }

    /// 检查ticket是否已被锁定(防止重复操作)
    /// 对应 JS: if (E && E[b.R]) return;
    pub async fn is_ticket_locked(&self, ticket: i32) -> bool {
        let locks = self.ticket_locks.read().await;
        locks.contains_key(&ticket)
    }

    /// 添加待确认请求
    /// 对应 JS: E[b.R] = b.kj; N[b.kj] = b;
    pub async fn add_pending(&self, request: TradeRequest) -> i32 {
        let request_id = request.request_id;
        let target_ticket = if request.ticket != 0 {
            Some(request.ticket)
        } else {
            None
        };

        // 如果是针对特定ticket的操作，锁定该ticket
        if let Some(ticket) = target_ticket {
            let mut locks = self.ticket_locks.write().await;
            locks.insert(ticket, request_id);
        }

        // 添加到待确认队列
        let pending = PendingRequest {
            request_id,
            request,
            created_at: Instant::now(),
            target_ticket,
        };

        let mut pending_requests = self.pending_requests.write().await;
        pending_requests.insert(request_id, pending);

        request_id
    }

    /// 确认请求完成(收到响应后调用)
    /// 对应 JS 第1212行:
    /// - E[e.R] = null (清除ticket锁)
    /// - clearTimeout(W[c.Xg]) (清除超时)
    /// - N[c.Xg] = null (移除待确认)
    pub async fn confirm(&self, request_id: i32) -> Option<PendingRequest> {
        let mut pending_requests = self.pending_requests.write().await;
        if let Some(pending) = pending_requests.remove(&request_id) {
            // 清除ticket锁
            if let Some(ticket) = pending.target_ticket {
                let mut locks = self.ticket_locks.write().await;
                // 只有当锁对应的request_id匹配时才清除
                if locks.get(&ticket) == Some(&request_id) {
                    locks.remove(&ticket);
                }
            }
            Some(pending)
        } else {
            None
        }
    }

    /// 获取超时的请求 (超过指定时间未确认)
    /// 对应 JS 第1183行的超时处理: setTimeout(..., 180000)
    pub async fn get_timed_out(&self, timeout_secs: u64) -> Vec<PendingRequest> {
        let pending_requests = self.pending_requests.read().await;
        let now = Instant::now();
        pending_requests
            .values()
            .filter(|p| now.duration_since(p.created_at).as_secs() >= timeout_secs)
            .cloned()
            .collect()
    }

    /// 移除超时的请求并返回
    pub async fn remove_timed_out(&self, timeout_secs: u64) -> Vec<PendingRequest> {
        let mut pending_requests = self.pending_requests.write().await;
        let mut locks = self.ticket_locks.write().await;
        let now = Instant::now();

        let timed_out: Vec<i32> = pending_requests
            .iter()
            .filter(|(_, p)| now.duration_since(p.created_at).as_secs() >= timeout_secs)
            .map(|(id, _)| *id)
            .collect();

        let mut result = Vec::new();
        for request_id in timed_out {
            if let Some(pending) = pending_requests.remove(&request_id) {
                // 清除ticket锁
                if let Some(ticket) = pending.target_ticket {
                    if locks.get(&ticket) == Some(&request_id) {
                        locks.remove(&ticket);
                    }
                }
                result.push(pending);
            }
        }
        result
    }

    /// 获取所有待确认请求数量
    pub async fn pending_count(&self) -> usize {
        self.pending_requests.read().await.len()
    }

    /// 清空所有追踪状态 (断开连接时调用)
    /// 对应 JS 第1216行的 B.hb() 函数
    pub async fn clear(&self) {
        self.pending_requests.write().await.clear();
        self.ticket_locks.write().await.clear();
    }
}

/// 客户端事件
#[derive(Debug, Clone)]
pub enum Mt4Event {
    /// 连接成功
    Connected,
    /// 认证成功
    Authenticated,
    /// 认证失败
    AuthFailed(u8),
    /// 账户信息
    AccountInfo(AccountInfo),
    /// 订单更新（实时推送，Command 10）- 单个订单
    OrderUpdate(OrderUpdate),
    /// 批量订单更新（实时推送，Command 10）- 多个订单一起推送
    /// MT4 对冲平仓等操作会一次性推送多个订单更新
    OrderUpdates(Vec<OrderUpdate>),
    /// 持仓快照（Command 4 响应，包含所有当前持仓）
    /// 用于同步本地缓存：不在快照中的订单应被移除
    PositionsSnapshot(Vec<Order>),
    /// 历史订单（Command 5 响应，包含已平仓订单）
    /// 这些订单不应触发跟单逻辑，仅用于显示和导出
    HistoryOrders(Vec<Order>),
    /// 交易成功
    TradeSuccess { request_id: i32, status: i32 },
    /// 交易失败
    TradeFailed { code: u8, message: String },
    /// 交易请求超时
    /// 根据 JS mt4.en.js 第1183行: 180秒超时生成 status=128 (Trade timeout)
    TradeTimeout {
        request_id: i32,
        request: TradeRequest,
        elapsed_secs: f64,
    },
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
    /// 请求追踪器 (用于管理待确认请求、防重复、超时)
    /// 根据 JS mt4.en.js 第1216行: N={}, W={}, E={}, B.GH=1000
    request_tracker: Arc<RequestTracker>,
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
            request_tracker: Arc::new(RequestTracker::new()),
        }
    }

    /// 获取请求追踪器的引用
    pub fn request_tracker(&self) -> &Arc<RequestTracker> {
        &self.request_tracker
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

        // 验证服务器是否匹配（API 可能返回不同的服务器）
        if token_info.trade_server != credentials.server {
            tracing::warn!(
                "⚠️ 服务器不匹配! 请求: {}, API返回: {}",
                credentials.server,
                token_info.trade_server
            );
            return Err(Mt4Error::Server(format!(
                "服务器配置错误: 账户 {} 属于服务器 {}，而非 {}",
                credentials.login,
                token_info.trade_server,
                credentials.server
            )));
        }

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
        let login_id: i32 = credentials.login.parse().unwrap_or(0);
        let token = token_info.token.clone();
        let write_tx_clone = write_tx.clone();
        let request_tracker = self.request_tracker.clone();
        let timeout_event_tx = event_tx.clone(); // 用于超时任务

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

                        tracing::info!(
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
                                    // 不发送 command=5，因为那是获取订单历史，不是当前持仓
                                    // 当前持仓通过 command=10 (OrderUpdate) 推送事件获取
                                } else {
                                    tracing::error!("Authentication failed: {}", error_code);
                                    let _ = event_tx.send(Mt4Event::AuthFailed(error_code)).await;
                                }
                            }
                            3 => {
                                // 账户信息响应
                                // 数据结构 (根据 JS 源码 line 1180):
                                // - 0-253: 账户信息 (254 字节, q.Vp=254)
                                // - 254-1161: 品种信息 (28字节*32个, parsed by Ur())
                                // - 1162+: 报价信息 (parsed by Qr() at offset q.Dk=1162)
                                // 注意: Command 3 不包含订单数据!
                                // 当前持仓需要通过 Command 4 请求, 历史订单通过 Command 5 获取

                                if let Some(mut account) = Self::parse_account_info(&msg_data) {
                                    // 使用认证时的 login (响应中可能没有正确的 login)
                                    account.login = login_id;
                                    tracing::info!(
                                        "Account: login={}, balance={:.2}, equity={:.2}, leverage={}",
                                        account.login,
                                        account.balance,
                                        account.equity,
                                        account.leverage
                                    );
                                    let _ = event_tx.send(Mt4Event::AccountInfo(account)).await;

                                    // 根据 mt4.en.js line 1181: 收到 Command 3 后调用 C.F.$().lf()
                                    // lf() 函数 (line 1216) 会发送 Command 4 请求获取当前持仓
                                    tracing::info!("Account info received, requesting current positions (Command 4)...");
                                    let crypto_guard = crypto.lock().await;
                                    if let Ok(packet) = Self::build_packet(
                                        Command::CurrentPositions as u16,
                                        &[],
                                        &crypto_guard,
                                        false,
                                    ) {
                                        drop(crypto_guard);
                                        if let Err(e) = write_tx_clone.send(packet).await {
                                            tracing::error!("Failed to send Command 4 request: {}", e);
                                        }
                                    }

                                } else {
                                    tracing::warn!(
                                        "Failed to parse AccountInfo: data_len={}",
                                        msg_data.len()
                                    );
                                    let _ = event_tx.send(Mt4Event::RawMessage {
                                        command,
                                        error_code,
                                        data: msg_data,
                                    }).await;
                                }
                            }
                            4 => {
                                // 当前持仓订单列表 (Command 4, mb.Mm)
                                // 根据 mt4.en.js line 1204 函数 D 和 line 1296 的 Oo() 函数：
                                // - 这是初始化 ef[] 数组（当前持仓）的命令
                                // - 数据格式: 161 字节 Order 结构数组（无头部）
                                // - 使用 Sr() 函数解析 (Math.floor(byteLength/161))
                                // - 每个订单调用 Oo() 添加到 ef[] 数组

                                let mut orders = Vec::new();

                                // 记录原始数据长度和 error_code，便于诊断
                                tracing::info!(
                                    "Command 4 响应: error_code={}, data_len={} 字节",
                                    error_code,
                                    msg_data.len()
                                );

                                if msg_data.is_empty() {
                                    tracing::warn!("Command 4 (当前持仓): 空数据 (无持仓订单或服务器未返回)");
                                } else {
                                    let order_count = msg_data.len() / 161;
                                    tracing::info!(
                                        "Command 4 (当前持仓): {} 个订单 ({} 字节)",
                                        order_count,
                                        msg_data.len()
                                    );

                                    for i in 0..order_count {
                                        let offset = i * 161;
                                        if let Some(order) = Order::from_bytes(&msg_data, offset) {
                                            // tracing::info!(
                                            //     "持仓 #{}: ticket={}, symbol={}, type={:?}, volume={:.2}, open={:.5}, profit={:.2}",
                                            //     i,
                                            //     order.ticket,
                                            //     order.symbol,
                                            //     order.order_type,
                                            //     order.volume,
                                            //     order.open_price,
                                            //     order.profit
                                            // );
                                            orders.push(order);
                                        }
                                    }
                                }

                                // 发送持仓快照事件（包含所有当前持仓，用于同步本地缓存）
                                let _ = event_tx.send(Mt4Event::PositionsSnapshot(orders)).await;
                            }
                            5 => {
                                // 订单历史响应或当前持仓响应
                                tracing::info!(
                                    "Command 5 response: data_len={} bytes",
                                    msg_data.len()
                                );

                                // 输出 hex 数据以便分析
                                if !msg_data.is_empty() {
                                    // // 输出前 200 字节
                                    // let hex_preview = msg_data.iter()
                                    //     .take(200)
                                    //     .map(|b| format!("{:02x}", b))
                                    //     .collect::<Vec<_>>()
                                    //     .join(" ");
                                    // tracing::info!("Command 5 data (first 200 bytes): {}", hex_preview);

                                    // // 输出前 3 个 161 字节记录的完整 hex
                                    // for i in 0..3 {
                                    //     let offset = i * 161;
                                    //     if msg_data.len() >= offset + 161 {
                                    //         let order_hex = msg_data[offset..offset+161].iter()
                                    //             .map(|b| format!("{:02x}", b))
                                    //             .collect::<Vec<_>>()
                                    //             .join(" ");
                                    //         tracing::info!("Record #{} (161 bytes): {}", i, order_hex);
                                    //     }
                                    // }

                                    // 解析订单（命令 5 = 历史订单）
                                    // 根据 mt4.en.js line 1103 的 Sr() 函数:
                                    // 数据格式: 161 字节 Order 结构数组（无头部）
                                    let order_count = msg_data.len() / 161;
                                    tracing::info!("Command 5: parsing {} orders from {} bytes", order_count, msg_data.len());

                                    let mut history_orders = Vec::with_capacity(order_count);
                                    for i in 0..order_count {
                                        let offset = i * 161;
                                        if let Some(order) = Order::from_bytes(&msg_data, offset) {
                                            // tracing::info!(
                                            //     "历史订单 #{}: ticket={}, symbol={}, type={:?}, volume={:.2}, open={:.5}, close={:.5}, profit={:.2}, open_time={}, close_time={}",
                                            //     i, order.ticket, order.symbol, order.order_type, order.volume,
                                            //     order.open_price, order.close_price, order.profit,
                                            //     order.open_time, order.close_time
                                            // );


                                            history_orders.push(order);
                                        }
                                    }

                                    // 一次性发送所有历史订单（使用新的 HistoryOrders 事件）
                                    if !history_orders.is_empty() {
                                        tracing::info!("Command 5: 发送 {} 个历史订单到引擎", history_orders.len());
                                        let _ = event_tx.send(Mt4Event::HistoryOrders(history_orders)).await;
                                    }
                                }
                            }
                            10 => {
                                // 订单更新 (实时推送) - 可能包含多个订单更新
                                // tracing::debug!(
                                //     "Order update raw: data_len={}, data_hex={:02x?}",
                                //     msg_data.len(),
                                //     &msg_data[..msg_data.len().min(32)]
                                // );

                                // 解析所有订单更新（一条消息可能包含多个）
                                let updates = OrderUpdate::parse_all(&msg_data);
                                if updates.is_empty() {
                                    tracing::warn!(
                                        "Failed to parse OrderUpdate: data_len={} (expected >= 185)",
                                        msg_data.len()
                                    );
                                } else {
                                    tracing::debug!("Parsed {} order update(s) from {} bytes", updates.len(), msg_data.len());
                                    for update in &updates {
                                        // tracing::info!(
                                        //     "Order update: ticket={}, symbol={}, type={:?}, notify_type={}, close_time={}, comment={}",
                                        //     update.order.ticket,
                                        //     update.order.symbol,
                                        //     update.order.order_type,
                                        //     update.notify_type,
                                        //     update.order.close_time,
                                        //     update.order.comment
                                        // );
                                        tracing::info!("update.order 详情: {:?}", update.order);

                                    }
                                    // 批量发送订单更新事件，让接收方可以一次性处理所有更新后再做决策 
                                    let _ = event_tx.send(Mt4Event::OrderUpdates(updates)).await;
                                }
                            }
                            12 => {
                                // 交易响应 - 解析完整的响应数据
                                // 根据 JS mt4.en.js 第1211行的 d 函数处理响应
                                if let Some(response) = crate::types::TradeResponse::from_bytes(&msg_data) {
                                    let request_id = response.request_id;

                                    // 详细日志：显示 error_code 和 response.status 的值
                                    tracing::debug!(
                                        "Trade response: request_id={}, error_code={}, response.status={}, price1={:.5}, price2={:.5}",
                                        request_id, error_code, response.status, response.price1, response.price2
                                    );

                                    // 确认请求完成 (对应 JS: clearTimeout(W[c.Xg]); N[c.Xg]=null; E[e.R]=null;)
                                    if let Some(pending) = request_tracker.confirm(request_id).await {
                                        tracing::info!(
                                            "📥 [响应确认] request_id={}, 耗时={:.2}秒, target_ticket={:?}",
                                            request_id,
                                            pending.created_at.elapsed().as_secs_f64(),
                                            pending.target_ticket
                                        );
                                    } else {
                                        tracing::warn!(
                                            "⚠️ [响应未匹配] request_id={} 未在待确认队列中找到",
                                            request_id
                                        );
                                    }

                                    // 根据JS原始逻辑:
                                    // - error_code > 0 只是通讯层警告,仍需检查response.status
                                    // - response.status >= 2 才是真正的交易错误
                                    // - response.status 0=Success, 1=Request sent (都表示成功/待确认)

                                    // 先记录通讯层警告(如果有)
                                    if error_code != 0 {
                                        let err = Mt4Error::from_trade_code(error_code);
                                        if let Mt4Error::Trade { code: _, message } = err {
                                            tracing::warn!(
                                                "Trade response with header error_code (warning only): request_id={}, error_code={}, response.status={}, msg={}",
                                                request_id, error_code, response.status, message
                                            );
                                        }
                                    }

                                    // 根据response.status判断交易结果
                                    if response.status >= 2 {
                                        // status >= 2 才是真正的错误
                                        let err = Mt4Error::from_trade_code(response.status as u8);
                                        if let Mt4Error::Trade { code, message } = err {
                                            tracing::warn!(
                                                "Trade failed (status>=2): request_id={}, error_code={}, response.status={}, code={}, msg={}",
                                                request_id, error_code, response.status, code, message
                                            );
                                            let _ = event_tx.send(Mt4Event::TradeFailed { code, message }).await;
                                        }
                                    } else {
                                        // status=0 (Success) 或 status=1 (Request sent) 都是成功/待确认
                                        tracing::info!(
                                            "Trade success (status=0 or 1): request_id={}, error_code={}, response.status={}, price1={:.5}, price2={:.5}, orders_count={}",
                                            request_id, error_code, response.status, response.price1, response.price2, response.orders.len()
                                        );
                                        let _ = event_tx.send(Mt4Event::TradeSuccess {
                                            request_id,
                                            status: response.status
                                        }).await;
                                    }
                                } else {
                                    tracing::error!("Failed to parse trade response, data_len={}", msg_data.len());
                                    // 如果解析失败，使用旧的简单解析方式作为后备
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

                                    // 确认请求完成
                                    if request_id != 0 {
                                        request_tracker.confirm(request_id).await;
                                    }

                                    // 根据JS原始逻辑: error_code只是警告,status>=2才是错误
                                    if error_code != 0 {
                                        let err = Mt4Error::from_trade_code(error_code);
                                        if let Mt4Error::Trade { code: _, message } = err {
                                            tracing::warn!("Trade response with header error_code (warning only): error_code={}, msg={}", error_code, message);
                                        }
                                    }

                                    if status >= 2 {
                                        let err = Mt4Error::from_trade_code(status as u8);
                                        if let Mt4Error::Trade { code, message } = err {
                                            tracing::warn!("Trade failed (status>=2): code={}, msg={}", code, message);
                                            let _ = event_tx.send(Mt4Event::TradeFailed { code, message }).await;
                                        }
                                    } else {
                                        tracing::info!("Trade success: request_id={}, status={}", request_id, status);
                                        let _ = event_tx.send(Mt4Event::TradeSuccess { request_id, status }).await;
                                    }
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

        // 9. 启动超时检测任务
        // 根据 JS mt4.en.js 第1183行: setTimeout(..., 180000) - 180秒超时
        let timeout_tracker = self.request_tracker.clone();
        tokio::spawn(async move {
            const TIMEOUT_SECS: u64 = 180; // 与 JS 一致
            const CHECK_INTERVAL_SECS: u64 = 5; // 每5秒检查一次

            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(CHECK_INTERVAL_SECS)
            );

            loop {
                interval.tick().await;

                // 获取超时的请求
                let timed_out = timeout_tracker.remove_timed_out(TIMEOUT_SECS).await;

                for pending in timed_out {
                    tracing::warn!(
                        "⏰ [请求超时] request_id={}, 等待时间={:.1}秒, symbol={}, ticket={}, 超过{}秒未响应",
                        pending.request_id,
                        pending.created_at.elapsed().as_secs_f64(),
                        pending.request.symbol,
                        pending.request.ticket,
                        TIMEOUT_SECS
                    );

                    // 发送超时事件
                    // 对应 JS: c.Yg = z.dn (status=128, Trade timeout)
                    let _ = timeout_event_tx.send(Mt4Event::TradeTimeout {
                        request_id: pending.request_id,
                        request: pending.request.clone(),
                        elapsed_secs: pending.created_at.elapsed().as_secs_f64(),
                    }).await;

                    // 同时发送 TradeFailed 事件 (与 JS 行为一致)
                    let _ = timeout_event_tx.send(Mt4Event::TradeFailed {
                        code: 128, // Trade timeout
                        message: "Trade timeout".to_string(),
                    }).await;
                }
            }
        });

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

    /// 发送交易请求 (内部方法，不使用追踪)
    async fn send_trade_internal(&self, request: &TradeRequest) -> Result<()> {
        let data = request.to_bytes();
        self.send_command(Command::TradeRequest, &data).await
    }

    /// 发送交易请求 (带追踪)
    /// 根据 JS mt4.en.js 第1183行的 J 函数:
    /// 1. 生成 request_id
    /// 2. 检查 ticket 防重复 (如果是针对特定ticket的操作)
    /// 3. 添加到待确认队列
    /// 4. 发送请求
    ///
    /// 返回 (request_id, is_duplicate)
    /// - request_id: 分配的请求ID
    /// - is_duplicate: 如果是重复操作则返回true (不发送)
    pub async fn send_trade(&self, mut request: TradeRequest) -> Result<(i32, bool)> {
        // 1. 生成 request_id (对应 JS: b.kj = B.GH++)
        let request_id = self.request_tracker.next_id();
        request.request_id = request_id;

        // 2. 检查 ticket 防重复 (对应 JS: if (E && E[b.R]) return;)
        if request.ticket != 0 {
            if self.request_tracker.is_ticket_locked(request.ticket).await {
                tracing::warn!(
                    "⚠️ [请求跳过] ticket #{} 已有待确认操作，跳过重复请求 (request_id={})",
                    request.ticket,
                    request_id
                );
                return Ok((request_id, true)); // 重复操作
            }
        }

        tracing::info!(
            "📤 [发送请求] request_id={}, type={}, {:?} {} {} lots @ {}, ticket={}",
            request_id,
            request.trade_type,
            request.order_type,
            request.symbol,
            request.volume,
            request.price,
            request.ticket
        );

        // 3. 添加到待确认队列 (对应 JS: N[b.kj] = b; E[b.R] = b.kj;)
        self.request_tracker.add_pending(request.clone()).await;

        // 4. 发送请求
        let result = self.send_trade_internal(&request).await;

        if let Err(ref e) = result {
            // 发送失败，从待确认队列移除
            tracing::error!("❌ [发送失败] request_id={}: {}", request_id, e);
            self.request_tracker.confirm(request_id).await;
        }

        result.map(|_| (request_id, false))
    }

    /// 发送交易请求 (简化版，兼容旧接口)
    /// 返回 Result<()>，隐藏 request_id 和重复检测
    pub async fn send_trade_simple(&self, request: TradeRequest) -> Result<()> {
        let (_, is_duplicate) = self.send_trade(request).await?;
        if is_duplicate {
            // 对于简化接口，重复操作视为成功（已有请求在处理中）
            Ok(())
        } else {
            Ok(())
        }
    }

    /// 市价买入
    pub async fn buy(&self, symbol: &str, volume: f64, sl: Option<f64>, tp: Option<f64>) -> Result<()> {
        let request = TradeRequest::buy(symbol, volume, sl.unwrap_or(0.0), tp.unwrap_or(0.0));
        self.send_trade_simple(request).await
    }

    /// 市价卖出
    pub async fn sell(&self, symbol: &str, volume: f64, sl: Option<f64>, tp: Option<f64>) -> Result<()> {
        let request = TradeRequest::sell(symbol, volume, sl.unwrap_or(0.0), tp.unwrap_or(0.0));
        self.send_trade_simple(request).await
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
        self.send_trade_simple(request).await
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
        self.send_trade_simple(request).await
    }

    /// 平仓 (需要传入原订单方向，以便发送反向平仓)
    pub async fn close_order(&self, ticket: i32, symbol: &str, volume: f64) -> Result<()> {
        let request = TradeRequest::close(ticket, symbol, volume);
        tracing::info!(
            "Sending close: ticket={}, symbol={}, volume={}",
            ticket, symbol, volume
        );
        self.send_trade_simple(request).await
    }

    /// 取消挂单
    pub async fn cancel_order(&self, ticket: i32, symbol: &str) -> Result<()> {
        let request = TradeRequest::cancel(ticket, symbol);
        tracing::info!("Sending cancel: ticket={}, symbol={}", ticket, symbol);
        self.send_trade_simple(request).await
    }

    /// 发送 Ping
    pub async fn ping(&self) -> Result<()> {
        self.send_command(Command::Ping, &[]).await
    }

    /// 请求账户信息
    pub async fn request_account_info(&self) -> Result<()> {
        self.send_command(Command::AccountInfo, &[]).await
    }

    /// 请求订单历史（注意：这是历史记录，不是当前持仓）
    /// 当前持仓通过 command=10 (OrderUpdate) 推送事件获取
    ///
    /// # 参数
    /// - `start_time`: 可选的开始时间（Unix时间戳，秒）
    /// - `end_time`: 可选的结束时间（Unix时间戳，秒）
    ///
    /// 如果不提供时间范围，将返回所有历史订单
    /// 请求当前持仓列表 (Command 4)
    ///
    /// 根据 mt4.en.js line 1216 的 B.lf() 函数:
    /// - 这个请求会初始化 JavaScript 中的 ef[] 数组（当前持仓）
    /// - 在收到 Command 3 (账户信息) 后自动调用
    /// - 无需参数，服务器返回所有当前持仓订单
    pub async fn request_current_positions(&self) -> Result<()> {
        tracing::info!("Requesting current positions (Command 4)...");
        self.send_command(Command::CurrentPositions, &[]).await
    }

    pub async fn request_order_history(&self) -> Result<()> {
        self.send_command(Command::OrdersRequest, &[]).await
    }

    /// 请求指定时间范围的订单历史
    ///
    /// # 参数
    /// - `start_time`: 开始时间（Unix时间戳，秒）
    /// - `end_time`: 结束时间（Unix时间戳，秒）
    ///
    /// # 示例
    /// ```rust
    /// // 获取最近7天的订单
    /// let now = std::time::SystemTime::now()
    ///     .duration_since(std::time::UNIX_EPOCH)
    ///     .unwrap()
    ///     .as_secs() as i32;
    /// let seven_days_ago = now - 7 * 24 * 3600;
    /// client.request_order_history_range(seven_days_ago, now).await?;
    /// ```
    pub async fn request_order_history_range(&self, start_time: i32, end_time: i32) -> Result<()> {
        // 构造8字节的数据包
        // 前4字节: 开始时间（Unix时间戳，秒）
        // 后4字节: 结束时间（Unix时间戳，秒）
        let mut data = Vec::with_capacity(8);
        data.extend_from_slice(&start_time.to_le_bytes());
        data.extend_from_slice(&end_time.to_le_bytes());

        self.send_command(Command::OrdersRequest, &data).await
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

    /// 解析账户信息响应 (command=3)
    ///
    /// 数据包结构 (根据 JS 源码):
    /// - 账户信息头部 (约 254 字节，q.Vp=254)
    /// - 品种信息 (254-1161)
    /// - 报价信息 (1162+, q.Dk=1162)
    fn parse_account_info(data: &[u8]) -> Option<AccountInfo> {
        AccountInfo::from_bytes(data)
    }

    /// 判断订单是否已平仓
    ///
    /// 判断逻辑:
    /// 1. close_time > 0 表示已平仓 (最可靠)
    /// 2. close_price > 0 且 != open_price 表示已平仓 (备用)
    fn is_order_closed(order: &Order) -> bool {
        // 方法1: 有明确的平仓时间
        if order.close_time > 0 {
            return true;
        }

        // 方法2: close_price 有意义且不等于开仓价 (允许浮点误差)
        if order.close_price > 0.0 && (order.close_price - order.open_price).abs() > 0.00001 {
            return true;
        }

        false
    }
}

impl Default for Mt4Client {
    fn default() -> Self {
        Self::new()
    }
}

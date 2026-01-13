# MT4 WebSocket Client Library

MetaTrader 4 Web Terminal WebSocket 协议的 Rust 实现。

## 目录

- [协议概述](#协议概述)
- [安装](#安装)
- [快速开始](#快速开始)
- [API 文档](#api-文档)
- [协议详解](#协议详解)
- [数据结构](#数据结构)
- [错误码](#错误码)

---

## 协议概述

MT4 Web Terminal 使用以下通信流程：

```
┌─────────┐                              ┌─────────────┐                    ┌─────────────┐
│  客户端  │                              │  HTTP 服务器 │                    │  WS 服务器   │
└────┬────┘                              └──────┬──────┘                    └──────┬──────┘
     │                                          │                                  │
     │  1. POST /trade/json                     │                                  │
     │     (login, server, gwt)                 │                                  │
     │ ────────────────────────────────────────>│                                  │
     │                                          │                                  │
     │  2. 返回: token, key, signal_server      │                                  │
     │ <────────────────────────────────────────│                                  │
     │                                          │                                  │
     │  3. WebSocket 连接                                                          │
     │ ───────────────────────────────────────────────────────────────────────────>│
     │                                                                             │
     │  4. 发送 Token (Command 0, AuthKey 加密)                                     │
     │ ───────────────────────────────────────────────────────────────────────────>│
     │                                                                             │
     │  5. Token 确认                                                               │
     │ <───────────────────────────────────────────────────────────────────────────│
     │                                                                             │
     │  6. 发送密码 (Command 1, SessionKey 加密, UTF-16 LE)                          │
     │ ───────────────────────────────────────────────────────────────────────────>│
     │                                                                             │
     │  7. 认证成功/失败                                                            │
     │ <───────────────────────────────────────────────────────────────────────────│
     │                                                                             │
     │  8. 交易/数据请求 (SessionKey 加密)                                          │
     │ <──────────────────────────────────────────────────────────────────────────>│
     │                                                                             │
```

### 关键要点

1. **HTTP 请求不包含密码** - 密码仅通过 WebSocket 发送
2. **双密钥加密**:
   - `AuthKey`: 预设密钥，仅用于 Token 认证
   - `SessionKey`: 服务器返回，用于后续所有通信
3. **密码格式**: UTF-16 LE 编码，64 字节
4. **加密算法**: AES-256-CBC，零 IV

---

## 安装

### Cargo.toml

```toml
[dependencies]
mt4_client = { path = "path/to/mt4-rust" }
# 或使用 git
# mt4_client = { git = "https://github.com/your/repo.git" }

tokio = { version = "1", features = ["full"] }
```

---

## 快速开始

```rust
use mt4_client::{Mt4Client, Mt4Event, LoginCredentials};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 创建客户端
    let mut client = Mt4Client::new();

    // 2. 连接
    let credentials = LoginCredentials {
        login: "31313724".to_string(),
        password: "your_password".to_string(),
        server: "ICMarketsSC-Demo03".to_string(),
    };
    client.connect(&credentials).await?;

    // 3. 等待认证
    while let Some(event) = client.next_event().await {
        match event {
            Mt4Event::Authenticated => {
                println!("登录成功!");
                break;
            }
            Mt4Event::AuthFailed(code) => {
                eprintln!("登录失败: {}", code);
                return Ok(());
            }
            _ => {}
        }
    }

    // 4. 下单
    client.buy("EURUSD", 0.01, None, None).await?;

    // 5. 处理事件
    while let Some(event) = client.next_event().await {
        match event {
            Mt4Event::OrderUpdate(update) => {
                println!("订单更新: #{} {} {:.5}",
                    update.order.ticket,
                    update.order.symbol,
                    update.order.open_price);
            }
            Mt4Event::TradeSuccess { request_id, status } => {
                println!("交易成功: request_id={}, status={}", request_id, status);
            }
            _ => {}
        }
    }

    Ok(())
}
```

---

## API 文档

### Mt4Client

#### 连接管理

```rust
// 创建客户端
let mut client = Mt4Client::new();

// 连接并认证
client.connect(&credentials).await?;

// 检查连接状态
if client.is_connected() { ... }

// 断开连接
client.disconnect().await;
```

#### 交易操作

```rust
// 市价买入
// buy(symbol, lots, sl, tp)
client.buy("EURUSD", 0.01, None, None).await?;
client.buy("EURUSD", 0.1, Some(1.1000), Some(1.1200)).await?;

// 市价卖出
client.sell("EURUSD", 0.01, None, None).await?;

// 限价买入
// buy_limit(symbol, lots, price, sl, tp)
client.buy_limit("EURUSD", 0.01, 1.0800, None, None).await?;

// 限价卖出
client.sell_limit("EURUSD", 0.01, 1.1200, None, None).await?;

// 平仓
// close_order(ticket, symbol, volume)
client.close_order(12345678, "EURUSD", 0.01).await?;
```

#### 数据请求

```rust
// 请求账户信息
client.request_account_info().await?;

// 请求订单历史 (所有历史订单)
client.request_order_history().await?;

// 请求指定时间范围的订单历史
let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i32;

// 获取最近7天的订单
let seven_days_ago = now - 7 * 24 * 3600;
client.request_order_history_range(seven_days_ago, now).await?;

// 获取最近24小时的订单
let one_day_ago = now - 24 * 3600;
client.request_order_history_range(one_day_ago, now).await?;

// 发送心跳 (每30秒调用一次)
client.ping().await?;
```

#### 事件处理

```rust
// 接收下一个事件
while let Some(event) = client.next_event().await {
    match event {
        Mt4Event::Authenticated => { /* 认证成功 */ }
        Mt4Event::AuthFailed(code) => { /* 认证失败 */ }
        Mt4Event::OrderUpdate(update) => { /* 订单更新 */ }
        Mt4Event::TradeSuccess { request_id, status } => { /* 交易成功 */ }
        Mt4Event::TradeFailed { code, message } => { /* 交易失败 */ }
        Mt4Event::Pong => { /* 心跳响应 */ }
        Mt4Event::Disconnected => { /* 连接断开 */ }
        Mt4Event::Error(msg) => { /* 错误 */ }
        Mt4Event::RawMessage { command, error_code, data } => { /* 原始消息 */ }
        _ => {}
    }
}
```

---

## 协议详解

### 1. HTTP Token 请求

**端点**: `POST https://metatraderweb.app/trade/json`

**请求体** (application/x-www-form-urlencoded):
```
login=31313724&trade_server=ICMarketsSC-Demo03&gwt=4
```

**响应**:
```json
{
  "signal_server": "gwt4.mql5.com:443",
  "trade_server": "ICMarketsSC-Demo03",
  "login": "31313724",
  "company": "Raw Trading Ltd",
  "key": "1d4cdf97065ca0043e1606a75396fe894f7ca154b6b4d140438fb97363f5f858",
  "token": "hkjc8b57CvkRwcy2vH5qnZkxMvfzSsXUst5A3qyEihshzXMVji4mLgdaaDiLNp75",
  "enabled": true,
  "ssl": true
}
```

| 字段 | 说明 |
|------|------|
| signal_server | WebSocket 服务器地址 |
| key | 会话密钥 (64位十六进制 = 32字节) |
| token | 认证令牌 |
| enabled | 是否支持 Web Terminal |

### 2. 加密机制

#### AuthKey (预设密钥)

原始编码值:
```
"13ef13b2b76dd8:5795gdcfb2fdc1ge85bf768f54773d22fff996e3ge75g5:75"
```

解码步骤:
1. 每个字符 `charCode - 1`
2. Hex 解码

解码后 (32字节):
```
02de02a1a65cc794684fcbea1ecb0fd74ae657e43662c11eee885d2fd64f4964
```

#### SessionKey

服务器返回的 `key` 字段，直接 hex 解码为 32 字节。

#### 加密算法

- 算法: AES-256-CBC
- IV: 16 字节零值
- 填充: PKCS7

### 3. 消息格式

#### 发送格式

```
┌──────────────────┬──────────────────┬─────────────────────────────┐
│  Length (4字节)  │  Type (4字节)    │  Encrypted Payload          │
│  Little Endian   │  固定值 1        │  AES-256-CBC 加密           │
└──────────────────┴──────────────────┴─────────────────────────────┘
```

Payload 结构 (加密前):
```
┌──────────────────┬──────────────────┬───────────────────────────────┐
│  Random (2字节)  │  Command (2字节) │  Data                         │
│  随机值          │  Little Endian   │  命令相关数据                  │
└──────────────────┴──────────────────┴───────────────────────────────┘
```

#### 接收格式

```
┌──────────────────┬──────────────────┬─────────────────────────────┐
│  Length (4字节)  │  Type (4字节)    │  Encrypted Payload          │
└──────────────────┴──────────────────┴─────────────────────────────┘
```

Payload 结构 (解密后):
```
┌──────────────────┬──────────────────┬──────────────┬───────────────┐
│  Random (2字节)  │  Command (2字节) │  Error (1字节)│  Data         │
└──────────────────┴──────────────────┴──────────────┴───────────────┘
```

### 4. 命令 ID

| ID | 名称 | 方向 | 说明 |
|----|------|------|------|
| 0 | AUTH_TOKEN | 发送 | 发送 Token (64字节 ASCII) |
| 1 | AUTH_PASSWORD | 发送/接收 | 发送密码 (64字节 UTF-16 LE) / 认证响应 |
| 2 | LOGOUT | 发送 | 登出 |
| 3 | ACCOUNT_INFO | 发送/接收 | 请求/接收账户信息 |
| 5 | ORDERS_REQUEST | 发送 | 请求订单历史 (可选时间范围，见下方说明) |
| 10 | ORDER_UPDATE | 接收 | 订单更新通知 (185字节) |
| 11 | CHART_REQUEST | 发送 | K线历史请求 |
| 12 | TRADE_REQUEST | 发送/接收 | 交易请求/响应 |
| 51 | PING | 发送/接收 | 心跳 |

#### Command 5 (ORDERS_REQUEST) 数据格式

**无参数格式** (返回所有历史订单):
```
无数据，仅发送命令 ID
```

**时间范围格式** (8字节):
```
Offset  Size  Type     Field         说明
------  ----  -------  -----------   ----------------------
0       4     i32      start_time    开始时间 (Unix时间戳，秒)
4       4     i32      end_time      结束时间 (Unix时间戳，秒)
```

**示例**:
```rust
// 无参数 - 获取所有历史订单
client.request_order_history().await?;

// 带时间范围 - 只获取指定时间段的订单
let start = 1704067200;  // 2024-01-01 00:00:00
let end = 1735689600;    // 2025-01-01 00:00:00
client.request_order_history_range(start, end).await?;
```

### 5. 交易请求格式 (95字节)

```
Offset  Size  Type     Field       说明
------  ----  -------  ----------  ----------------------
0       1     u8       type        交易类型 (见下表)
1       2     i16      cmd         订单类型 (0=BUY, 1=SELL, ...)
3       4     i32      ticket      订单号 (新订单为0)
7       4     i32      unknown     保留字段
11      12    char[]   symbol      品种 (ASCII)
23      4     i32      volume      手数 * 100
27      8     f64      price       价格 (市价单为0)
35      8     f64      sl          止损
43      8     f64      tp          止盈
51      4     i32      slippage    滑点
55      32    char[]   comment     注释 (UTF-8)
87      4     i32      expiration  过期时间
91      4     i32      unknown     保留字段
```

**交易类型 (type)**:

| 值 | 名称 | 说明 |
|----|------|------|
| 0 | Quote | 报价请求 |
| 64 | Instant | 立即执行 |
| 65 | Request | 请求执行 |
| 66 | Market | 市价执行 |
| 67 | Pending | 挂单 |
| 68 | CloseInstant | 立即平仓 |
| 70 | CloseMarket | 市价平仓 |
| 71 | Modify | 修改订单 |
| 72 | Delete | 删除订单 |

**订单类型 (cmd)**:

| 值 | 名称 |
|----|------|
| 0 | BUY |
| 1 | SELL |
| 2 | BUY_LIMIT |
| 3 | SELL_LIMIT |
| 4 | BUY_STOP |
| 5 | SELL_STOP |

---

## 数据结构

### 订单更新 (Command 10)

订单更新有两种格式:

| 大小 | 类型 | 说明 |
|------|------|------|
| 185 字节 | 标准订单更新 | 单个订单的开仓/平仓/修改通知 |
| 370 字节 | 对冲平仓 (Close By) | 包含两个订单: 被平仓单 + 对冲单 |

#### 标准格式 (185字节)

```
Offset  Size  Type     Field         说明
------  ----  -------  -----------   ----------------------
0       4     i32      notify_id     通知 ID
4       4     i32      notify_type   通知类型 (1=更新)
8       16    -        reserved      保留

--- 订单数据 (从 offset 24 开始, 161字节) ---

24      4     i32      ticket        订单号
28      12    char[]   symbol        品种
40      4     i32      digits        小数位数
44      4     i32      cmd           订单类型
48      4     i32      volume        手数 * 100
52      4     i32      open_time     开仓时间 (Unix)
56      4     i32      state         状态
60      8     f64      open_price    开仓价
68      8     f64      sl            止损
76      8     f64      tp            止盈
84      4     i32      close_time    平仓时间 (0=持仓中)
88      4     i32      expiration    过期时间
92      1     i8       unknown       -
93      8     f64      commission    佣金
101     8     f64      unknown       -
109     8     f64      swap          隔夜利息
117     8     f64      close_price   平仓价
125     8     f64      profit        盈亏
133     8     f64      unknown       -
141     4     i32      unknown       -
145     32    char[]   comment       注释 (UTF-8)
177     8     f64      unknown       -
```

#### 对冲平仓格式 (370字节)

当使用 "Close By" (对冲平仓) 操作时，服务器发送 370 字节的数据包:

```
┌─────────────────────────────────────────────────────────────────┐
│  Header (24字节)  │  Order 1 (161字节)  │  Order 2 (185字节)    │
│  notify_id/type   │  被平仓的订单        │  对冲订单的完整更新    │
└─────────────────────────────────────────────────────────────────┘
```

#### Rust 结构体

```rust
pub struct OrderUpdate {
    pub notify_id: i32,
    pub notify_type: i32,
    pub raw_size: usize,           // 原始数据包大小
    pub order: Order,              // 主订单
    pub related_order: Option<Order>, // 关联订单 (Close By 时存在)
}

impl OrderUpdate {
    /// 是否为平仓通知
    pub fn is_close_notification(&self) -> bool;

    /// 是否为对冲平仓 (Close By)
    pub fn is_close_by(&self) -> bool;
}
```

#### 使用示例

```rust
Mt4Event::OrderUpdate(update) => {
    if update.is_close_by() {
        println!("对冲平仓!");
        println!("订单1: #{}", update.order.ticket);
        if let Some(ref related) = update.related_order {
            println!("订单2: #{}", related.ticket);
        }
    } else if update.is_close_notification() {
        println!("订单已平仓: #{}", update.order.ticket);
    } else {
        println!("订单更新: #{}", update.order.ticket);
    }
}
```

---

## 错误码

### 认证错误 (Command 1 响应)

| 错误码 | 说明 |
|--------|------|
| 0 | 成功 |
| 64 | 账户已禁用 |
| 65 | 无效账户 |
| 66 | 公钥未找到 |

### 交易错误 (Command 12 响应)

| 错误码 | 说明 |
|--------|------|
| 0 | 成功 |
| 1 | 请求已发送 |
| 2 | 通用错误 |
| 3 | 无效参数 |
| 128 | 交易超时 |
| 129 | 无效价格 |
| 130 | 无效止损/止盈 |
| 131 | 无效手数 |
| 132 | 市场已关闭 |
| 133 | 交易已禁用 |
| 134 | 资金不足 |
| 135 | 价格已变动 |
| 136 | 无报价 |
| 138 | 重新报价 |
| 142 | 订单已接受 |
| 143 | 订单处理中 |
| 148 | 订单过多 |
| 149 | 禁止对冲 |

---

## 示例项目

### trade_test - 订单监控示例

运行测试:

```bash
cd mt4-rust
cargo run --example trade_test -- <login> <password> <server>

# 示例
cargo run --example trade_test -- 31313724 ufrt32 ICMarketsSC-Demo03
```

**功能特性**:
- 实时监控订单更新（持仓、平仓、修改等）
- CSV格式输出，便于数据分析和导入Excel
- 自动记录到 `orders.log` 文件
- 支持对冲平仓（Close By）订单显示

**CSV输出格式**:
```csv
时间,通知类型,订单号,品种,类型,手数,开仓价,平仓价,止损,止盈,盈亏,佣金,隔夜利息,开仓时间,平仓时间,注释
2025-12-29 10:28:09,持仓中/新订单,534483380,GBPUSD,Sell,0.00,1.34153,1.34153,0.00000,0.00000,1.34,0.00,0.00,1766398846,0,
2025-12-29 10:28:09,已平仓,534483381,EURUSD,Buy,0.01,1.05234,1.05256,0.00000,0.00000,2.20,0.00,0.00,1766398850,1766398900,
```

**字段说明**:
- 时间: 记录时间（YYYY-MM-DD HH:MM:SS）
- 通知类型: 持仓中/新订单、已平仓、订单修改、订单删除、对冲单
- 订单号: MT4订单号
- 品种: 交易品种（如GBPUSD、AUDUSD）
- 类型: 订单类型（Buy、Sell、BuyLimit、SellLimit等）
- 手数: 交易手数
- 开仓价/平仓价: 价格（5位小数）
- 止损/止盈: 止损止盈价格
- 盈亏/佣金/隔夜利息: 费用信息
- 开仓时间/平仓时间: Unix时间戳（秒）
- 注释: 订单备注

---

## 注意事项

1. **心跳**: 每 30 秒发送一次 PING (Command 51) 保持连接
2. **密码安全**: 密码仅通过加密的 WebSocket 传输，不经过 HTTP
3. **手数单位**: API 中使用实际手数 (如 0.01)，协议中使用 手数*100 (如 1)
4. **时间格式**: 所有时间戳为 Unix 时间戳 (秒)
5. **字节序**: 所有数值类型使用 Little Endian

---

## License

MIT

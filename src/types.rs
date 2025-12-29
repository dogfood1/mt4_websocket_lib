//! 数据类型定义

use crate::protocol::OrderType;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::Cursor;

/// 订单信息
#[derive(Debug, Clone)]
pub struct Order {
    /// 订单号
    pub ticket: i32,
    /// 品种
    pub symbol: String,
    /// 小数位数
    pub digits: i32,
    /// 订单类型
    pub order_type: OrderType,
    /// 手数 (实际手数，已除以100)
    pub volume: f64,
    /// 开仓时间 (Unix 时间戳)
    pub open_time: i64,
    /// 开仓价格
    pub open_price: f64,
    /// 止损
    pub sl: f64,
    /// 止盈
    pub tp: f64,
    /// 平仓时间 (Unix 时间戳，0 表示未平仓)
    pub close_time: i64,
    /// 平仓价格
    pub close_price: f64,
    /// 佣金
    pub commission: f64,
    /// 隔夜利息
    pub swap: f64,
    /// 盈亏
    pub profit: f64,
    /// 注释
    pub comment: String,
}

impl Order {
    /// 从字节数据解析订单 (161字节)
    ///
    /// Order 数据结构 (161 bytes) - 根据 mt4.en.js 分析:
    /// - 0-3:     ticket (i32)
    /// - 4-15:    symbol (12 bytes ASCII)
    /// - 16-19:   digits (i32)
    /// - 20-23:   cmd/order_type (i32)
    /// - 24-27:   volume (i32, raw / 10000)
    /// - 28-31:   unknown1 (i32)
    /// - 32-35:   unknown2 (i32)
    /// - 36-43:   open_price (f64)
    /// - 44-51:   sl (f64)
    /// - 52-59:   tp (f64)
    /// - 60-63:   unknown3 (i32)
    /// - 64-67:   open_time (i32, seconds)
    /// - 68:      unknown4 (i8)
    /// - 69-76:   commission (f64)
    /// - 77-84:   unknown5 (f64)
    /// - 85-92:   swap (f64)
    /// - 93-100:  profit (f64)
    /// - 101-108: unknown6 (f64)
    /// - 109-116: unknown7 (f64)
    /// - 117-120: unknown8 (i32)
    /// - 121-152: comment (32 bytes UTF-8)
    /// - 153-160: close_price (f64)
    pub fn from_bytes(data: &[u8], offset: usize) -> Option<Self> {
        if data.len() < offset + 161 {
            return None;
        }

        // 使用直接字节读取而不是 cursor，以便精确控制偏移
        let base = offset;

        let ticket = i32::from_le_bytes([
            data[base], data[base+1], data[base+2], data[base+3]
        ]);

        // 读取 symbol (12字节 ASCII)
        let symbol_bytes = &data[base+4..base+16];
        let symbol = String::from_utf8_lossy(symbol_bytes)
            .trim_end_matches('\0')
            .to_string();

        let digits = i32::from_le_bytes([
            data[base+16], data[base+17], data[base+18], data[base+19]
        ]);

        let cmd = i32::from_le_bytes([
            data[base+20], data[base+21], data[base+22], data[base+23]
        ]);

        let volume_raw = i32::from_le_bytes([
            data[base+24], data[base+25], data[base+26], data[base+27]
        ]);

        // 跳过 unknown1 (28-31) 和 unknown2 (32-35)

        let open_price = f64::from_le_bytes([
            data[base+36], data[base+37], data[base+38], data[base+39],
            data[base+40], data[base+41], data[base+42], data[base+43],
        ]);

        let sl = f64::from_le_bytes([
            data[base+44], data[base+45], data[base+46], data[base+47],
            data[base+48], data[base+49], data[base+50], data[base+51],
        ]);

        let tp = f64::from_le_bytes([
            data[base+52], data[base+53], data[base+54], data[base+55],
            data[base+56], data[base+57], data[base+58], data[base+59],
        ]);

        // open_time: 开仓时间 (Unix时间戳，秒)
        // JavaScript中使用 c.zo = f.getInt32(k+28, !0)
        let open_time = i32::from_le_bytes([
            data[base+28], data[base+29], data[base+30], data[base+31]
        ]) as i64;

        // 跳过 unknown3 (32-35, 60-67)

        // 跳过 unknown4 (68)

        let commission = f64::from_le_bytes([
            data[base+69], data[base+70], data[base+71], data[base+72],
            data[base+73], data[base+74], data[base+75], data[base+76],
        ]);

        // 跳过 unknown5 (77-84)

        let swap = f64::from_le_bytes([
            data[base+85], data[base+86], data[base+87], data[base+88],
            data[base+89], data[base+90], data[base+91], data[base+92],
        ]);

        let profit = f64::from_le_bytes([
            data[base+93], data[base+94], data[base+95], data[base+96],
            data[base+97], data[base+98], data[base+99], data[base+100],
        ]);

        // 跳过 unknown6 (101-108), unknown7 (109-116), unknown8 (117-120)

        // comment (121-152, 32 bytes UTF-8)
        let comment_bytes = &data[base+121..base+153];
        let comment = String::from_utf8_lossy(comment_bytes)
            .trim_end_matches('\0')
            .to_string();

        let close_price = f64::from_le_bytes([
            data[base+153], data[base+154], data[base+155], data[base+156],
            data[base+157], data[base+158], data[base+159], data[base+160],
        ]);

        Some(Order {
            ticket,
            symbol,
            digits,
            order_type: OrderType::from_i32(cmd).unwrap_or(OrderType::Buy),
            volume: volume_raw as f64 / 10000.0,  // 正确的除数
            open_time,
            open_price,
            sl,
            tp,
            close_time: 0,  // close_time 不在这个结构中
            close_price,
            commission,
            swap,
            profit,
            comment,
        })
    }

    /// 是否为持仓订单
    pub fn is_open(&self) -> bool {
        self.close_time == 0
    }

    /// 是否为挂单
    pub fn is_pending(&self) -> bool {
        matches!(
            self.order_type,
            OrderType::BuyLimit
                | OrderType::SellLimit
                | OrderType::BuyStop
                | OrderType::SellStop
        )
    }
}

/// 交易请求
#[derive(Debug, Clone)]
pub struct TradeRequest {
    /// 请求类型
    pub trade_type: u8,
    /// 订单类型
    pub order_type: OrderType,
    /// 订单号 (新订单为0)
    pub ticket: i32,
    /// 品种
    pub symbol: String,
    /// 手数 (实际手数)
    pub volume: f64,
    /// 价格 (市价单可为0)
    pub price: f64,
    /// 止损
    pub sl: f64,
    /// 止盈
    pub tp: f64,
    /// 滑点
    pub slippage: i32,
    /// 注释
    pub comment: String,
    /// 过期时间
    pub expiration: i32,
}

impl TradeRequest {
    /// 创建市价买入请求
    pub fn buy(symbol: &str, volume: f64, sl: f64, tp: f64) -> Self {
        Self {
            trade_type: 66, // Market
            order_type: OrderType::Buy,
            ticket: 0,
            symbol: symbol.to_string(),
            volume,
            price: 0.0,
            sl,
            tp,
            slippage: 50,
            comment: String::new(),
            expiration: 0,
        }
    }

    /// 创建市价卖出请求
    pub fn sell(symbol: &str, volume: f64, sl: f64, tp: f64) -> Self {
        Self {
            trade_type: 66, // Market
            order_type: OrderType::Sell,
            ticket: 0,
            symbol: symbol.to_string(),
            volume,
            price: 0.0,
            sl,
            tp,
            slippage: 50,
            comment: String::new(),
            expiration: 0,
        }
    }

    /// 创建限价买入请求
    pub fn buy_limit(symbol: &str, volume: f64, price: f64, sl: f64, tp: f64) -> Self {
        Self {
            trade_type: 67, // Pending
            order_type: OrderType::BuyLimit,
            ticket: 0,
            symbol: symbol.to_string(),
            volume,
            price,
            sl,
            tp,
            slippage: 50,
            comment: String::new(),
            expiration: 0,
        }
    }

    /// 创建限价卖出请求
    pub fn sell_limit(symbol: &str, volume: f64, price: f64, sl: f64, tp: f64) -> Self {
        Self {
            trade_type: 67, // Pending
            order_type: OrderType::SellLimit,
            ticket: 0,
            symbol: symbol.to_string(),
            volume,
            price,
            sl,
            tp,
            slippage: 50,
            comment: String::new(),
            expiration: 0,
        }
    }

    /// 创建平仓请求
    pub fn close(ticket: i32, symbol: &str, volume: f64) -> Self {
        Self {
            trade_type: 70, // CloseMarket
            order_type: OrderType::Buy, // 会被忽略
            ticket,
            symbol: symbol.to_string(),
            volume,
            price: 0.0,
            sl: 0.0,
            tp: 0.0,
            slippage: 50,
            comment: String::new(),
            expiration: 0,
        }
    }

    /// 创建取消挂单请求
    pub fn cancel(ticket: i32, symbol: &str) -> Self {
        Self {
            trade_type: 72, // Delete
            order_type: OrderType::Buy, // 会被忽略
            ticket,
            symbol: symbol.to_string(),
            volume: 0.0,
            price: 0.0,
            sl: 0.0,
            tp: 0.0,
            slippage: 0,
            comment: String::new(),
            expiration: 0,
        }
    }

    /// 序列化为字节数组 (95字节)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buffer = vec![0u8; 95];
        let mut cursor = Cursor::new(&mut buffer[..]);

        // type (1 byte)
        cursor.write_u8(self.trade_type).unwrap();

        // cmd (2 bytes)
        cursor
            .write_i16::<LittleEndian>(self.order_type as i16)
            .unwrap();

        // ticket (4 bytes)
        cursor.write_i32::<LittleEndian>(self.ticket).unwrap();

        // unknown (4 bytes)
        cursor.write_i32::<LittleEndian>(0).unwrap();

        // symbol (12 bytes ASCII)
        let symbol_bytes = self.symbol.as_bytes();
        let len = symbol_bytes.len().min(12);
        buffer[11..11 + len].copy_from_slice(&symbol_bytes[..len]);

        // 跳过 symbol 后继续写入
        let mut cursor = Cursor::new(&mut buffer[23..]);

        // volume (4 bytes) - 手数*100
        cursor
            .write_i32::<LittleEndian>((self.volume * 100.0) as i32)
            .unwrap();

        // price (8 bytes)
        cursor.write_f64::<LittleEndian>(self.price).unwrap();

        // sl (8 bytes)
        cursor.write_f64::<LittleEndian>(self.sl).unwrap();

        // tp (8 bytes)
        cursor.write_f64::<LittleEndian>(self.tp).unwrap();

        // slippage (4 bytes)
        cursor.write_i32::<LittleEndian>(self.slippage).unwrap();

        // comment (32 bytes UTF-8)
        let comment_bytes = self.comment.as_bytes();
        let len = comment_bytes.len().min(32);
        buffer[55..55 + len].copy_from_slice(&comment_bytes[..len]);

        // expiration (4 bytes)
        let mut cursor = Cursor::new(&mut buffer[87..]);
        cursor.write_i32::<LittleEndian>(self.expiration).unwrap();

        // unknown (4 bytes)
        cursor.write_i32::<LittleEndian>(0).unwrap();

        buffer
    }
}

/// 账户信息
#[derive(Debug, Clone, Default)]
pub struct AccountInfo {
    /// 账号
    pub login: i32,
    /// 余额
    pub balance: f64,
    /// 净值
    pub equity: f64,
    /// 已用保证金
    pub margin: f64,
    /// 可用保证金
    pub free_margin: f64,
    /// 账户杠杆
    pub leverage: i32,
    /// 账户货币
    pub currency: String,
    /// 账户名称
    pub name: String,
    /// 服务器名称
    pub server: String,
    /// 公司名称
    pub company: String,
}

impl AccountInfo {
    /// 从字节数据解析账户信息
    ///
    /// 根据 MT4 Web Terminal JS 源码分析:
    /// 数据包格式: [4字节记录数] + [账户数据...]
    ///
    /// 账户数据结构 (从 offset 4 开始，即 base=4):
    /// - base+0:      1 byte  - flag
    /// - base+1:      8 bytes - balance (f64)
    /// - base+9:      8 bytes - equity (f64)
    /// - base+17:     32 bytes - currency (UTF-16 LE, 16 chars)
    /// - base+49:     4 bytes - login (u32)
    /// - base+53:     4 bytes - leverage (i32)
    /// - base+57:     1 byte  - unknown
    /// - base+58:     128 bytes - server (UTF-16 LE, 64 chars)
    /// - base+186:    2 bytes - unknown
    /// - base+188:    1 byte  - unknown
    /// - base+189:    1 byte  - unknown
    /// - base+190:    64 bytes - name (UTF-8)
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 260 {
            return None;
        }

        // 根据实际数据分析，消息头不是 4 字节
        // 数据格式 (JS 中的偏移直接对应 msg_data):
        // offset 0: flag (1 byte)
        // offset 1-8: balance (f64) - 但实际数据可能不在这里
        // offset 17: currency (UTF-16 LE) - "USD" 确认在这里
        // offset 49: leverage = 500 确认在这里
        // offset 57: unknown
        // offset 58: server (UTF-16 LE) - "ICMarketsSC-Demo03" 确认在这里

        // flag at offset 0
        let _flag = data[0];

        // currency at offset 17 (32 bytes UTF-16 LE = 16 chars)
        let currency = Self::read_utf16_string(data, 17, 16).unwrap_or_default();

        // leverage at offset 49 (4 bytes i32)
        let leverage = i32::from_le_bytes([data[49], data[50], data[51], data[52]]);

        // server at offset 58 (128 bytes UTF-16 LE = 64 chars)
        let server = Self::read_utf16_string(data, 58, 64).unwrap_or_default();

        // name at offset 190 (64 bytes UTF-8)
        let name = Self::read_ascii_string(data, 190, 64).unwrap_or_default();

        // balance 和 equity 需要找到正确位置
        // 根据 hex: 00 20 6e c3 40 00 00 00 在 offset 4-11
        // 这可能是某种编码的数值，让我们尝试不同的解析方式

        // 尝试从 offset 1 读取 balance (按 JS 代码)
        let balance = Self::read_f64(data, 1).unwrap_or(0.0);
        let equity = Self::read_f64(data, 9).unwrap_or(0.0);

        // login 需要搜索
        // MT4 账号通常是 7-8 位数字，范围 1,000,000 - 99,999,999
        let login = Self::find_login_value(data).unwrap_or(0);

        let margin = 0.0;
        let free_margin = 0.0;
        let company = String::new();

        Some(AccountInfo {
            login,
            balance,
            equity,
            margin,
            free_margin,
            leverage,
            currency,
            name,
            server,
            company,
        })
    }

    /// 在数据中搜索 MT4 账号值
    /// MT4 账号通常是 7-8 位数字
    fn find_login_value(data: &[u8]) -> Option<i32> {
        // 首先检查可能的固定偏移位置
        // 根据 JS 分析，login 可能在 offset 53 或其他位置
        let possible_offsets = [53, 49, 254, 255, 256, 257];

        for &offset in &possible_offsets {
            if data.len() >= offset + 4 {
                let val = i32::from_le_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                ]);
                // 检查是否是有效的 MT4 账号 (7-8 位数字)
                if val >= 1_000_000 && val <= 99_999_999 {
                    return Some(val);
                }
            }
        }

        // 如果固定偏移没找到，扫描整个数据
        for i in 0..data.len().saturating_sub(4) {
            let val = i32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
            // MT4 账号通常是 7-8 位数字
            if val >= 1_000_000 && val <= 99_999_999 {
                return Some(val);
            }
        }
        None
    }

    /// 读取 f64
    fn read_f64(data: &[u8], offset: usize) -> Option<f64> {
        if data.len() < offset + 8 {
            return None;
        }
        let mut cursor = Cursor::new(&data[offset..]);
        cursor.read_f64::<LittleEndian>().ok()
    }

    /// 读取 UTF-16 LE 字符串
    fn read_utf16_string(data: &[u8], offset: usize, max_chars: usize) -> Option<String> {
        if data.len() < offset + max_chars * 2 {
            return None;
        }

        let bytes = &data[offset..offset + max_chars * 2];
        let mut chars = Vec::new();

        for i in (0..bytes.len()).step_by(2) {
            let code = u16::from_le_bytes([bytes[i], bytes[i + 1]]);
            if code == 0 {
                break;
            }
            if let Some(c) = char::from_u32(code as u32) {
                chars.push(c);
            }
        }

        if chars.is_empty() {
            None
        } else {
            Some(chars.into_iter().collect())
        }
    }

    /// 从数据中读取 ASCII/UTF-8 字符串
    fn read_ascii_string(data: &[u8], offset: usize, max_len: usize) -> Option<String> {
        if data.len() < offset + max_len {
            return None;
        }
        let bytes = &data[offset..offset + max_len];
        let s = String::from_utf8_lossy(bytes)
            .trim_end_matches('\0')
            .to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

/// 报价数据
#[derive(Debug, Clone)]
pub struct Quote {
    /// 品种
    pub symbol: String,
    /// 买价
    pub bid: f64,
    /// 卖价
    pub ask: f64,
    /// 时间戳
    pub time: i64,
}

/// 交易响应 (Command 12)
#[derive(Debug, Clone)]
pub struct TradeResponse {
    /// 请求ID
    pub request_id: i32,
    /// 状态码 (0=成功, 1=请求已发送, >=2=错误)
    pub status: i32,
    /// 价格1 (bid或ask)
    pub price1: f64,
    /// 价格2 (bid或ask)
    pub price2: f64,
    /// 返回的订单数据 (交易成功时可能包含1-4个订单)
    pub orders: Vec<Order>,
}

impl TradeResponse {
    /// 从字节数据解析交易响应
    ///
    /// 数据格式 (根据 mt4.en.js q.xC 函数):
    /// - 0-3:     request_id (i32)
    /// - 4-7:     status (i32)
    /// - 8-15:    price1 (f64)
    /// - 16-23:   price2 (f64)
    /// - 24+:     订单数据 (每个161字节，最多4个)
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 24 {
            return None;
        }

        let request_id = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let status = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);

        let price1 = f64::from_le_bytes([
            data[8], data[9], data[10], data[11],
            data[12], data[13], data[14], data[15],
        ]);

        let price2 = f64::from_le_bytes([
            data[16], data[17], data[18], data[19],
            data[20], data[21], data[22], data[23],
        ]);

        // 解析订单数据 (每个161字节，最多4个)
        let mut orders = Vec::new();
        let mut offset = 24;

        while offset + 161 <= data.len() {
            // Order::from_bytes 的第二个参数是订单数据在data切片中的起始位置
            // 因为我们已经切片了，所以传0即可
            if let Some(order) = Order::from_bytes(&data[offset..offset + 161], 0) {
                orders.push(order);
            }
            offset += 161;
        }

        Some(TradeResponse {
            request_id,
            status,
            price1,
            price2,
            orders,
        })
    }
}

/// 订单更新事件
#[derive(Debug, Clone)]
pub struct OrderUpdate {
    /// 通知ID
    pub notify_id: i32,
    /// 通知类型 (0=新订单, 1=已平仓, 2=订单修改, 3=账户更新)
    pub notify_type: i32,
    /// 账户余额相关数据 (对应 JS 中的 df 字段，用于更新账户信息)
    pub df: f64,
    /// 账户信用相关数据 (对应 JS 中的 xh 字段，用于更新账户信息)
    pub xh: f64,
    /// 数据包原始大小
    pub raw_size: usize,
    /// 订单信息
    pub order: Order,
    /// 关联订单 (close by 时的对冲订单)
    pub related_order: Option<Order>,
}

impl OrderUpdate {
    /// 从字节数据解析
    ///
    /// 数据包格式:
    /// - 185 字节: 标准订单更新 (24字节头 + 161字节订单)
    ///   - 0-3: notify_id (4字节)
    ///   - 4-7: notify_type (4字节)
    ///   - 8-15: df (8字节 f64) - 账户余额相关数据
    ///   - 16-23: xh (8字节 f64) - 账户信用相关数据
    ///   - 24-184: Order数据 (161字节)
    /// - 370 字节: Close By 平仓 (24字节头 + 161字节订单1 + 185字节订单2更新)
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let raw_size = data.len();

        // 至少需要 24 字节头（包含 notify_id, notify_type, df, xh）
        if raw_size < 24 {
            return None;
        }

        let mut cursor = Cursor::new(data);
        let notify_id = cursor.read_i32::<LittleEndian>().ok()?;
        let notify_type = cursor.read_i32::<LittleEndian>().ok()?;
        let df = cursor.read_f64::<LittleEndian>().ok()?;
        let xh = cursor.read_f64::<LittleEndian>().ok()?;

        // 370 字节: Close By 平仓 (包含两个订单)
        // 格式: 24字节头 + 161字节(被平仓订单) + 185字节(对冲订单完整更新)
        if raw_size >= 370 {
            let order = Order::from_bytes(data, 24)?;
            // 第二个订单从偏移 24 + 161 = 185 开始
            // 但第二个订单是完整的 OrderUpdate 格式（185字节），订单数据从其偏移24开始
            // 所以实际订单数据在: 24 + 161 + 24 = 209
            let related_order = Order::from_bytes(data, 24 + 161 + 24);

            return Some(OrderUpdate {
                notify_id,
                notify_type,
                df,
                xh,
                raw_size,
                order,
                related_order,
            });
        }

        // 185 字节: 标准订单更新
        if raw_size >= 185 {
            if let Some(order) = Order::from_bytes(data, 24) {
                return Some(OrderUpdate {
                    notify_id,
                    notify_type,
                    df,
                    xh,
                    raw_size,
                    order,
                    related_order: None,
                });
            }
        }

        // 尝试从偏移8解析订单 (紧凑格式 - 向后兼容)
        if raw_size >= 8 + 161 {
            if let Some(order) = Order::from_bytes(data, 8) {
                return Some(OrderUpdate {
                    notify_id,
                    notify_type,
                    df: 0.0,
                    xh: 0.0,
                    raw_size,
                    order,
                    related_order: None,
                });
            }
        }

        None
    }

    /// 从数据中解析所有订单更新（一条消息可能包含多个订单更新）
    pub fn parse_all(data: &[u8]) -> Vec<OrderUpdate> {
        let mut results = Vec::new();
        let mut offset = 0;
        let total_len = data.len();

        while offset < total_len {
            let remaining = &data[offset..];
            let remaining_len = remaining.len();

            if remaining_len < 185 {
                // 不足以解析一个完整的订单更新
                break;
            }

            // 尝试解析一个 OrderUpdate
            if let Some(update) = Self::from_bytes(remaining) {
                let consumed = if update.raw_size >= 370 {
                    // Close By 格式：370 字节（包含两个订单）
                    370
                } else if update.raw_size >= 185 {
                    // 标准格式：185 字节
                    185
                } else {
                    // 紧凑格式：实际大小
                    update.raw_size as usize
                };

                results.push(update);
                offset += consumed;
            } else {
                // 解析失败，尝试跳过 185 字节继续
                offset += 185;
            }
        }

        results
    }

    /// 是否为平仓通知
    pub fn is_close_notification(&self) -> bool {
        self.order.close_time > 0
    }

    /// 是否为 Close By 操作 (对冲平仓)
    pub fn is_close_by(&self) -> bool {
        self.raw_size >= 370 && self.related_order.is_some()
    }

    /// 获取实际的平仓价格
    ///
    /// 根据 JavaScript 源码分析：
    /// - df 和 xh 用于更新账户余额信息（m.I.df=d.df, m.I.xh=d.xh），不是订单价格
    /// - 订单的平仓价格始终存储在 order.close_price 字段中
    /// - JavaScript 显示平仓价格时使用 g.Tc (对应 order.close_price)
    pub fn get_actual_close_price(&self) -> f64 {
        self.order.close_price
    }
}

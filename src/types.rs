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
    pub fn from_bytes(data: &[u8], offset: usize) -> Option<Self> {
        if data.len() < offset + 161 {
            return None;
        }

        let mut cursor = Cursor::new(&data[offset..]);

        let ticket = cursor.read_i32::<LittleEndian>().ok()?;

        // 读取 symbol (12字节 ASCII)
        let mut symbol_bytes = [0u8; 12];
        std::io::Read::read_exact(&mut cursor, &mut symbol_bytes).ok()?;
        let symbol = String::from_utf8_lossy(&symbol_bytes)
            .trim_end_matches('\0')
            .to_string();

        let digits = cursor.read_i32::<LittleEndian>().ok()?;
        let cmd = cursor.read_i32::<LittleEndian>().ok()?;
        let volume_raw = cursor.read_i32::<LittleEndian>().ok()?;
        let open_time = cursor.read_i32::<LittleEndian>().ok()? as i64;
        let _state = cursor.read_i32::<LittleEndian>().ok()?;
        let open_price = cursor.read_f64::<LittleEndian>().ok()?;
        let sl = cursor.read_f64::<LittleEndian>().ok()?;
        let tp = cursor.read_f64::<LittleEndian>().ok()?;
        let close_time = cursor.read_i32::<LittleEndian>().ok()? as i64;
        let _expiration = cursor.read_i32::<LittleEndian>().ok()?;
        let _unknown = cursor.read_i8().ok()?;
        let commission = cursor.read_f64::<LittleEndian>().ok()?;
        let _unknown2 = cursor.read_f64::<LittleEndian>().ok()?;
        let swap = cursor.read_f64::<LittleEndian>().ok()?;
        let close_price = cursor.read_f64::<LittleEndian>().ok()?;
        let profit = cursor.read_f64::<LittleEndian>().ok()?;

        // 跳过到 comment 位置 (offset 121)
        let comment_start = offset + 121;
        let comment_bytes = &data[comment_start..comment_start + 32];
        let comment = String::from_utf8_lossy(comment_bytes)
            .trim_end_matches('\0')
            .to_string();

        Some(Order {
            ticket,
            symbol,
            digits,
            order_type: OrderType::from_i32(cmd).unwrap_or(OrderType::Buy),
            volume: volume_raw as f64 / 100.0,
            open_time,
            open_price,
            sl,
            tp,
            close_time,
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

/// 订单更新事件
#[derive(Debug, Clone)]
pub struct OrderUpdate {
    /// 通知ID
    pub notify_id: i32,
    /// 通知类型
    pub notify_type: i32,
    /// 订单信息
    pub order: Order,
}

impl OrderUpdate {
    /// 从字节数据解析 (185字节)
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 185 {
            return None;
        }

        let mut cursor = Cursor::new(data);
        let notify_id = cursor.read_i32::<LittleEndian>().ok()?;
        let notify_type = cursor.read_i32::<LittleEndian>().ok()?;

        // 订单数据从偏移24开始
        let order = Order::from_bytes(data, 24)?;

        Some(OrderUpdate {
            notify_id,
            notify_type,
            order,
        })
    }
}

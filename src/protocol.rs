//! MT4 WebSocket 协议常量和数据结构

/// 预设的认证密钥 (用于 token 加密)
/// 原始值: "13ef13b2b76dd8:5795gdcfb2fdc1ge85bf768f54773d22fff996e3ge75g5:75"
/// 解码方式: 每个字符 charCode - 1，然后 hex 解码
pub const AUTH_KEY_HEX: &str = "02de02a1a65cc794684fcbea1ecb0fd74ae657e43662c11eee885d2fd64f4964";

/// WebSocket 命令 ID
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Command {
    /// 发送 token (认证第一步)
    AuthToken = 0,
    /// 发送密码 (认证第二步)
    AuthPassword = 1,
    /// 登出
    Logout = 2,
    /// 请求账户信息
    AccountInfo = 3,
    /// 请求当前持仓 (Command 4, mt4.en.js Mm)
    /// 对应 JavaScript 中的 ef[] 数组初始化
    CurrentPositions = 4,
    /// 请求历史订单 (Command 5, mt4.en.js Km)
    OrdersRequest = 5,
    /// 请求历史记录
    HistoryRequest = 6,
    /// 报价请求
    QuotesRequest = 8,
    /// 历史订单
    HistoryOrders = 9,
    /// 订单更新通知
    OrderUpdate = 10,
    /// K线历史请求
    ChartRequest = 11,
    /// 交易请求
    TradeRequest = 12,
    /// 平仓请求
    CloseOrder = 13,
    /// 连接状态
    ConnectionStatus = 15,
    /// 修改订单
    ModifyOrder = 16,
    /// 订阅报价
    QuoteSubscribe = 26,
    /// 报价历史
    QuoteHistory = 27,
    /// 断开连接
    Disconnect = 28,
    /// 取消订单
    CancelOrder = 29,
    /// Ping 心跳
    Ping = 51,
}

impl Command {
    /// 从 u16 创建命令
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            0 => Some(Command::AuthToken),
            1 => Some(Command::AuthPassword),
            2 => Some(Command::Logout),
            3 => Some(Command::AccountInfo),
            4 => Some(Command::CurrentPositions),
            5 => Some(Command::OrdersRequest),
            6 => Some(Command::HistoryRequest),
            8 => Some(Command::QuotesRequest),
            9 => Some(Command::HistoryOrders),
            10 => Some(Command::OrderUpdate),
            11 => Some(Command::ChartRequest),
            12 => Some(Command::TradeRequest),
            13 => Some(Command::CloseOrder),
            15 => Some(Command::ConnectionStatus),
            16 => Some(Command::ModifyOrder),
            26 => Some(Command::QuoteSubscribe),
            27 => Some(Command::QuoteHistory),
            28 => Some(Command::Disconnect),
            29 => Some(Command::CancelOrder),
            51 => Some(Command::Ping),
            _ => None,
        }
    }
}

/// 订单类型 (cmd)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(i32)]
pub enum OrderType {
    Buy = 0,
    Sell = 1,
    BuyLimit = 2,
    SellLimit = 3,
    BuyStop = 4,
    SellStop = 5,
}

impl OrderType {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(OrderType::Buy),
            1 => Some(OrderType::Sell),
            2 => Some(OrderType::BuyLimit),
            3 => Some(OrderType::SellLimit),
            4 => Some(OrderType::BuyStop),
            5 => Some(OrderType::SellStop),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            OrderType::Buy => "BUY",
            OrderType::Sell => "SELL",
            OrderType::BuyLimit => "BUY_LIMIT",
            OrderType::SellLimit => "SELL_LIMIT",
            OrderType::BuyStop => "BUY_STOP",
            OrderType::SellStop => "SELL_STOP",
        }
    }
}

/// 交易请求类型 (type)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TradeType {
    /// 报价请求
    Quote = 0,
    /// 立即执行
    Instant = 64,
    /// 请求执行
    Request = 65,
    /// 市价执行
    Market = 66,
    /// 挂单
    Pending = 67,
    /// 立即平仓
    CloseInstant = 68,
    /// 请求平仓
    CloseRequest = 69,
    /// 市价平仓
    CloseMarket = 70,
    /// 修改订单
    Modify = 71,
    /// 删除订单
    Delete = 72,
}

/// 消息包装结构
#[derive(Debug)]
pub struct Message {
    pub command: u16,
    pub error_code: u8,
    pub data: Vec<u8>,
}

/// 交易请求大小 (95字节)
pub const TRADE_REQUEST_SIZE: usize = 95;

/// 订单数据大小 (161字节)
pub const ORDER_DATA_SIZE: usize = 161;

/// 订单更新通知大小 (185字节)
pub const ORDER_UPDATE_SIZE: usize = 185;

/// Token/Password 大小 (64字节)
pub const AUTH_DATA_SIZE: usize = 64;

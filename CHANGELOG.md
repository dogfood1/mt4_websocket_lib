# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **修正订单获取逻辑的重大错误** (基于 mt4.en.js 深度分析):
  - **Command 3 错误解析**: 之前错误地尝试从 Command 3 响应中解析订单
    - **正确**: Command 3 只包含账户信息 (0-253)、品种信息 (254-1161)、报价信息 (1162+)
    - **不包含订单数据!** (mt4.en.js line 1180)

  - **Command 4 才是当前持仓数据源** (重要发现!):
    - 通过分析 JavaScript 代码发现命令映射 (line 1123): `Mm:4` 对应 Command 4
    - Command 4 处理函数 D (line 1204) 调用 `Sr()` 解析订单，然后调用 `Oo()` 添加到 `ef[]` 数组
    - `ef[]` 是 MT4 Web Terminal 的**当前持仓数组**
    - 数据格式: 161 字节 Order 结构数组 (无头部)
    - **这是初始化当前持仓列表的命令!**

  - **完整的订单数据流** (通过 JavaScript 源码验证):
    - **Command 4**: 当前持仓初始化 (`ef[]` 数组) - **需要客户端主动请求!**
      - mt4.en.js line 1181: 收到 Command 3 后调用 `C.F.$().lf()`
      - mt4.en.js line 1216: `B.lf()` 函数调用 `a.Ze(a.mb.Mm)` 发送 Command 4 请求
      - **这是主动请求，而非服务器主动推送！**
    - **Command 5**: 历史订单查询 (`history[]` 数组) - 客户端主动请求
    - **Command 10**: 实时订单更新 (OrderUpdate) - 服务器实时推送

  - **修复内容**:
    - 移除 Command 3 中错误的订单解析代码
    - **实现 Command 4 自动请求**: 收到 Command 3 (账户信息) 后自动发送 Command 4 请求
      - 在消息处理器中直接构建并发送数据包（使用 `build_packet` + `write_tx_clone`）
      - 模仿 JavaScript 源码中的行为 (line 1181 → line 1216)
    - 完善 Command 4 处理逻辑，正确标记为当前持仓 (`notify_type=0`)
    - 修正 `protocol.rs` 中的 `from_u16()` 方法，将 `SymbolInfo` 更名为 `CurrentPositions`
    - 更新所有相关注释，注明 JavaScript 源码行号供参考
    - 新增 `request_current_positions()` 公共方法，允许手动请求当前持仓

## [0.3.0] - 2025-12-29

### Added

- **时间范围订单历史查询**: 新增 `request_order_history_range()` 方法，支持按时间范围获取历史订单
  - 参数：`start_time` 和 `end_time` (Unix时间戳，秒)
  - 数据包格式：8字节 (2个 i32 小端字节序)
  - 用途：减少数据传输量，提高查询效率
- 订单状态自动识别: 新增 `is_order_closed()` 内部方法，通过比较 `close_price` 和 `open_price` 判断订单是否已平仓
- **正确的 `notify_type` 含义** (基于 mt4.en.js 源码分析):
  - `notify_type=0`: 新订单 (New Order, 对应 JS 中的 T.su)
  - `notify_type=1`: 已平仓 (Close, 对应 JS 中的 T.Fw)
  - `notify_type=2`: 订单修改 (Modify, 对应 JS 中的 T.eG)
  - `notify_type=3`: 账户更新 (Account Update, 对应 JS 中的 T.Iu)
- **CSV格式订单输出**: `trade_test` 示例程序现支持CSV格式输出
  - 控制台输出CSV格式，便于复制到Excel
  - 包含完整的CSV表头
  - 16个字段：时间、通知类型、订单号、品种、类型、手数、开仓价、平仓价、止损、止盈、盈亏、佣金、隔夜利息、开仓时间、平仓时间、注释
  - 支持对冲单（Close By）显示
- **OrderUpdate df/xh 字段**: 新增 `df` 和 `xh` 字段（完整解析24字节头部）
  - **重要**: 通过 JavaScript 源码分析发现，这两个字段不是订单价格
  - 字段名直接使用 JavaScript 中的原始变量名（df 和 xh）以保持一致性
  - 用途：在平仓等事件发生时**更新账户余额信息**（`m.I.df=d.df, m.I.xh=d.xh`）
  - 订单的平仓价格始终存储在 `order.close_price` 字段中（JavaScript 使用 `g.Tc`）
  - `get_actual_close_price()` 方法直接返回 `order.close_price`

### Changed

- 更新示例代码 `trade_test.rs`：
  - 默认不自动获取历史订单，只监听实时订单更新
  - 移除调试日志输出，保持输出简洁
  - CSV格式输出，提高可读性和可分析性
- 保留原有 `request_order_history()` 方法，向后兼容（无参数，返回所有历史订单）

### Fixed

- 修复订单解析中的字段偏移问题：
  - `volume`: 使用正确的除数 10000（之前为 100）
  - `open_time`: 修正为偏移 28 字节（JavaScript中对应 `zo` 字段）
  - `profit`: 修正为偏移 93 字节
  - `close_price`: 修正为偏移 153 字节
- 修复 Command 5 返回的历史订单全部显示为"持仓中"的问题
- 修复历史订单 `open_time` 显示为 0 的问题（通过分析JavaScript源码，发现正确的字段位置在偏移28而非64）
- **修复 Command 12 (交易响应) 数据解析不完整的bug**：
  - 之前只解析了前8字节（request_id + status）
  - 现在完整解析所有数据：request_id (4字节) + status (4字节) + price1 (8字节) + price2 (8字节) + 订单数据 (161字节*N)
  - 新增 `TradeResponse` 结构体来存储完整的响应数据
  - 避免了未消费的数据导致后续消息解析错误的问题
- **修复 Command 10 (订单更新) 数据解析不完整的bug**：
  - **问题**：OrderUpdate 数据包前24字节包含头部数据，但之前只解析了前8字节
    - 导致后续的 Order 数据解析偏移错误
    - 订单字段值全部错位
  - **根本原因**：OrderUpdate 数据包前24字节包含 notify_id (4) + notify_type (4) + **df (8) + xh (8)**
    - 之前的代码完全忽略了 df 和 xh 这两个字段（共16字节）
    - Order 数据从偏移24开始，而非之前错误的偏移8
  - **解决方案**：
    - 在 `OrderUpdate` 结构体中添加 `df` 和 `xh` 字段（使用 JS 原始变量名）
    - 修改 `from_bytes()` 方法正确解析完整的24字节头部
    - 从正确的偏移量（24）开始解析 Order 数据
    - 通过 JavaScript 分析确认 df/xh 用于账户余额更新，而非订单价格
- **修正 notify_type 含义理解错误**：
  - **问题**：之前错误地认为 notify_type=2 代表平仓
  - **正确理解**：通过分析 mt4.en.js 源码发现 `T={su:0,Fw:1,eG:2,Iu:3}`，其中 Fw(1) 才是 Close
  - **修复**：
    - 更正所有代码注释和文档中的 notify_type 含义
    - 更新 `trade_test.rs` 状态显示逻辑：0=新订单, 1=已平仓, 2=订单修改, 3=账户更新

### Protocol Analysis

通过分析 `mt4.en.js` 源码，发现以下协议细节：

#### Command 5 (ORDERS_REQUEST) 时间范围参数

```javascript
// JavaScript 源码片段 (mt4.en.js)
q.nG=function(g){
    var k=new ArrayBuffer(8),
    f=new DataView(k);
    f.setInt32(0,Math.floor((g.$h||0)/1E3),!0);   // 开始时间
    f.setInt32(4,Math.floor((g.UH||0)/1E3),!0);   // 结束时间
    return k
}
```

#### Command 10 (ORDER_UPDATE) 完整数据格式

```javascript
// OrderUpdate 解析函数 (mt4.en.js:1103)
function J(g,k){
    k||(k=0);
    var f=new DataView(g),c={};
    c.tY=f.getUint32(k,!0);          // offset 0: notify_id (4字节)
    c.$H=f.getInt32(k+=4,!0);        // offset 4: notify_type (4字节)
    c.df=f.getFloat64(k+=4,!0);      // offset 8: price1 (8字节) - 实际成交价
    c.xh=f.getFloat64(k+=8,!0);      // offset 16: price2 (8字节) - 市场价
    c.R=w(g.slice(k+8));             // offset 24: Order数据 (161字节)
    return c
}
```

**数据包结构**：
- **总大小**: 185 字节（标准格式）或 370 字节（Close By 格式）
- **前24字节头部**:
  - 0-3: notify_id (i32)
  - 4-7: notify_type (i32) - **0=新订单, 1=已平仓, 2=修改, 3=账户更新**
  - **8-15: df (f64)** - 账户余额相关数据（用于更新账户信息 m.I.df）
  - **16-23: xh (f64)** - 账户信用相关数据（用于更新账户信息 m.I.xh）
- **24-184**: Order 结构体 (161字节)

### Examples

```rust
// 获取最近7天的订单
let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i32;
let seven_days_ago = now - 7 * 24 * 3600;
client.request_order_history_range(seven_days_ago, now).await?;

// 获取所有历史订单 (向后兼容)
client.request_order_history().await?;
```

## [0.2.0] - 2024-12-24

### Added

- **Close By (对冲平仓) 支持**: 正确解析 370 字节的对冲平仓数据包
- `OrderUpdate.related_order` 字段: 存储关联的对冲订单信息
- `OrderUpdate.raw_size` 字段: 记录原始数据包大小，便于调试
- `OrderUpdate.is_close_by()` 方法: 判断是否为对冲平仓操作
- `OrderUpdate.is_close_notification()` 方法: 判断是否为平仓通知

### Changed

- **Breaking Change**: `OrderUpdate` 结构体新增 `related_order: Option<Order>` 和 `raw_size: usize` 字段
- 改进订单更新日志输出，包含 `notify_type` 和 `close_time` 信息
- 支持多种数据包格式的解析 (185字节标准格式、370字节对冲平仓格式)

### Fixed

- 修复对冲平仓 (Close By) 时只能解析一个订单的问题
- 修复不同大小数据包导致解析失败的问题

### Data Packet Formats

| 大小 | 类型 | 结构 |
|------|------|------|
| 185 字节 | 标准订单更新 | 24字节头 + 161字节订单 |
| 370 字节 | 对冲平仓 | 24字节头 + 161字节订单1 + 185字节订单2 |

## [0.1.0] - 2024-12-24

### Added

- Initial release
- MT4 WebSocket 协议实现
- 支持认证、下单、平仓、查询订单等基本功能
- AES-256-CBC 加密通信
- 完整的错误码处理

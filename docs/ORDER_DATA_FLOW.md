# MT4 WebSocket 订单数据流分析

基于 mt4.en.js JavaScript 源码的深度分析，完整解析 MT4 Web Terminal 的订单数据获取机制。

## 命令码映射 (mt4.en.js line 1123)

```javascript
a = {
    Rk:0,   // Command 0 -
    Pk:1,   // Command 1 -
    Lm:2,   // Command 2 -
    Lq:3,   // Command 3 - 账户信息 (Account Info)
    Mm:4,   // Command 4 - 当前持仓 (Current Positions) ★
    Km:5,   // Command 5 - 历史订单 (History Orders)
    Om:6,   // Command 6 -
    iB:7,   // Command 7 -
    Nq:8,   // Command 8 -
    Nm:9,   // Command 9 -
    jB:10,  // Command 10 - 订单更新 (Order Update) ★
    Qk:11,  // Command 11 -
    Oq:12,  // Command 12 - 交易响应 (Trade Response)
    Pm:13,  // Command 13 -
    ...
}
```

## 核心数据结构

### `ef[]` 数组 - 当前持仓 (Line 1296)

```javascript
u.ef = [];  // MT4 Web Terminal 的当前持仓数组

// 添加订单到 ef[] 的函数
u.Oo = function(g){
    if(g){
        var h=u.ef,k;
        if(h)
            // 检查是否已存在（根据 ticket）
            for(var d=g.R,b=0,e=h.length;b<e;b++)
                if(k=h[b],k.R==d) return;

        g.$l={};
        u.ef.push(g);  // 添加到当前持仓数组
        ...
    }
};
```

### `history[]` 数组 - 历史订单 (Line 1296)

```javascript
u.history = [];  // 历史订单数组
u.ro = [];       // 历史订单排序副本
```

## 订单数据流详解

### 1. Command 3 - 账户信息 (Line 1180)

**用途**: 登录后获取账户信息、品种列表、报价数据

**处理函数 `w()`**:
```javascript
function w(c,q){
    if(c==f.Gb.ee.gb){
        // 解析账户信息
        var w=v.A.Pr(q),y;
        for(y in w) g.I[y]=w[y];

        // 解析品种信息 (offset 254)
        if(w=v.F.Ur(q,v.A.Vp)){
            for(var p=w.length;y<p;y++)
                g.Ne.nn(y,w[y]);
        }

        // 解析报价信息 (offset 1162)
        if(q.byteLength>v.A.Dk&&(w=v.F.Qr(q,v.A.Dk)))
            for(var p=0,d=w.length;p<d;p++)
                (y=w[p])&&g.F.sC(y);

        // 注意：没有订单解析！
    }
}
```

**数据结构**:
- 0-253: 账户信息 (254字节, `q.Vp=254`)
- 254-1161: 品种信息 (28字节×32个品种)
- 1162+: 报价信息 (`q.Dk=1162`)
- **不包含订单数据！**

### 2. Command 4 - 当前持仓初始化 ★ (Line 1204)

**用途**: 登录后初始化 `ef[]` 数组，获取所有当前持仓

**重要**: 这是**客户端主动请求**，而非服务器主动推送！
- Line 1181: 收到 Command 3 后调用 `C.F.$().lf()`
- Line 1216: `B.lf()` 函数调用 `a.Ze(a.mb.Mm)` 发送 Command 4 请求

**处理函数 `D()`**:
```javascript
function D(b,c){
    var d=K.Gb;
    d.Xk(d.mb.Mm,D);  // 注册 Command 4 (Mm=4) 处理器

    if(b==K.Gb.ee.gb){
        // 使用 Sr() 解析订单
        for(var d=Q.A.Sr(c),f,g=0,h=d.length;g<h;g++)
            if(f=d[g])
                m.A.Oo(f),  // 调用 Oo() 添加到 ef[] 数组 ★
                k(f);

        C();
        a.A.da();
        m.U.v("change");
        ...
    }
}
```

**Sr() 解析函数** (Line 1103):
```javascript
q.Sr=function(g){
    if(!g) return[];
    // 简单按 161 字节分割
    for(var k=[],f,c=Math.floor(g.byteLength/161),h=0;h<c;h++)
        (f=w(g,161*h))&&(k[k.length]=f);
    return k
};
```

**数据格式**:
- 161 字节 × N 个订单
- 无头部，直接是 Order 结构数组
- 每个 Order 是 161 字节

**触发时机**:
- **客户端主动请求** (Line 1216)
- 收到 Command 3 (账户信息) 后，调用 `B.lf()` 发送请求
- 请求代码: `a.Ze(a.mb.Mm)` (Mm=4)
- 无需参数，空数据包

### 3. Command 5 - 历史订单查询 (Line 1205)

**用途**: 客户端主动请求历史订单

**处理函数 `y()`**:
```javascript
function y(b,c){
    var e=K.Gb;
    e.Xk(e.mb.Km,y);  // 注册 Command 5 (Km=5) 处理器

    if(b==K.Gb.ee.gb){
        e=Q.A.Sr(c);  // 同样使用 Sr() 解析
        m.A.history=e;
        m.A.ro=e.slice().sort(u);  // 排序副本
        a.History.da();
    }
}
```

**数据格式**:
- 与 Command 4 相同：161 字节 × N 个订单
- 无头部

**请求方法** (Line 1105):
```javascript
q.nG=function(g){
    var k=new ArrayBuffer(8),
    f=new DataView(k);
    // 开始时间 (Unix timestamp, 秒)
    f.setInt32(0,Math.floor((g.$h||0)/1E3),!0);
    // 结束时间 (Unix timestamp, 秒)
    f.setInt32(4,Math.floor((g.UH||0)/1E3),!0);
    return k
}
```

**触发时机**:
- 客户端主动调用 `request_order_history_range(start, end)`
- 或调用 `request_order_history()` (获取所有历史)

### 4. Command 10 - 订单实时更新 ★ (Line 1205)

**用途**: 服务器实时推送订单变化

**处理函数 `p()`**:
```javascript
function p(b,c){
    if(b==K.Gb.ee.gb){
        var e=Q.A.yC(c),d,g,h;  // 使用 yC() 解析 OrderUpdate
        if(e){
            for(var k=0,p=e.length;k<p;k++)
                if(d=e[k])
                    switch(g=d.R,h=d.$H,h){
                        case T.su:  // notify_type=0: 新订单
                            ...
                            m.A.Oo(g);  // 添加到 ef[]
                            break;

                        case T.Fw:  // notify_type=1: 已平仓
                            ...
                            m.A.zw(g);  // 从 ef[] 移除，添加到 history[]
                            break;

                        case T.eG:  // notify_type=2: 订单修改
                            ...
                            m.A.Rh(g);  // 更新现有订单
                            break;

                        case T.Iu:  // notify_type=3: 账户更新
                            m.I.df=d.df;
                            m.I.xh=d.xh;
                            break;
                    }
        }
    }
}
```

**yC() 解析函数** (Line 1103):
```javascript
q.yC=function(g){
    if(!g) return[];
    // 按 185 字节分割 OrderUpdate
    for(var k=[],f,c=Math.floor(g.byteLength/185),h=0;h<c;h++)
        (f=J(g,185*h))&&k.push(f);
    return k
};
```

**OrderUpdate 解析** (Line 1101):
```javascript
function J(g,k){
    k||(k=0);
    var f=new DataView(g),c={};
    c.tY=f.getUint32(k,!0);          // offset 0: notify_id (4字节)
    c.$H=f.getInt32(k+=4,!0);        // offset 4: notify_type (4字节)
    c.df=f.getFloat64(k+=4,!0);      // offset 8: df (8字节)
    c.xh=f.getFloat64(k+=8,!0);      // offset 16: xh (8字节)
    c.R=w(g.slice(k+8));             // offset 24: Order数据 (161字节)
    return c
}
```

**数据格式**:
- 185 字节 (标准) 或 370 字节 (Close By)
- 前 24 字节头部:
  - 0-3: notify_id
  - 4-7: notify_type (0=新订单, 1=已平仓, 2=修改, 3=账户更新)
  - 8-15: df (账户余额相关)
  - 16-23: xh (账户信用相关)
- 24-184: Order 结构 (161字节)

**触发时机**:
- 服务器实时推送
- 任何订单状态变化都会触发

## 完整数据流时序图

```
客户端连接
    ↓
发送认证请求 (Command 0: Token)
    ↓
发送密码 (Command 1: Password)
    ↓
接收认证响应 (Command 1, error_code=0 表示成功)
    ↓
接收账户信息 (Command 3)
  - 账户数据
  - 品种列表
  - 报价数据
  - ★ 触发 Command 4 请求 (mt4.en.js line 1181 → 1216)
    ↓
发送 Command 4 请求 ★★★
  - 调用 B.lf() → a.Ze(a.mb.Mm)
  - 空数据包（无参数）
    ↓
接收 Command 4 响应 ★★★
  - 初始化 ef[] 数组
  - 所有当前持仓订单 (161字节×N)
    ↓
[可选] 请求历史订单 (Command 5)
  - 填充 history[] 数组
    ↓
持续接收订单更新 (Command 10)
  - 新订单 → 添加到 ef[]
  - 平仓 → 从 ef[] 移到 history[]
  - 修改 → 更新 ef[] 中的订单
```

## Rust 实现对照

### Command 4 处理 (src/client.rs:236-283)

```rust
4 => {
    // 当前持仓订单列表 (Command 4, mb.Mm)
    let order_count = msg_data.len() / 161;

    for i in 0..order_count {
        let offset = i * 161;
        if let Some(order) = Order::from_bytes(&msg_data, offset) {
            let update = OrderUpdate {
                notify_id: 0,
                notify_type: 0,  // 0=新订单/当前持仓
                df: 0.0,
                xh: 0.0,
                raw_size: 161,
                order,
                related_order: None,
            };
            event_tx.send(Mt4Event::OrderUpdate(update)).await;
        }
    }
}
```

### Command 5 处理 (src/client.rs:284-345)

```rust
5 => {
    // 历史订单
    let order_count = msg_data.len() / 161;

    for i in 0..order_count {
        let offset = i * 161;
        if let Some(order) = Order::from_bytes(&msg_data, offset) {
            let is_closed = Self::is_order_closed(&order);
            let notify_type = if is_closed { 1 } else { 0 };

            let update = OrderUpdate {
                notify_id: 0,
                notify_type,
                df: 0.0,
                xh: 0.0,
                raw_size: 161,
                order,
                related_order: None,
            };
            event_tx.send(Mt4Event::OrderUpdate(update)).await;
        }
    }
}
```

### Command 10 处理 (src/client.rs:346+)

```rust
10 => {
    // 订单更新 (实时推送)
    // 185 字节或 370 字节 (Close By)
    let update = OrderUpdate::from_bytes(&msg_data);
    event_tx.send(Mt4Event::OrderUpdate(update)).await;
}
```

## B.lf() 函数详解 (Line 1216)

**Command 4 请求的关键函数**:

```javascript
// mt4.en.js line 1216
B.lf=function(){
    var a=K.Gb;
    a.Se(a.mb.Mm,D);   // 注册 Command 4 处理器 D
    a.Ze(a.mb.Mm);     // ★ 发送 Command 4 请求! (Mm=4)
    w(O.yj);
    return B
};
```

**调用链**:
1. Command 3 处理完成后调用 `C.F.$().lf()` (line 1181)
2. `lf()` 内部调用 `B.lf()` (line 1216)
3. `B.lf()` 调用 `a.Ze(a.mb.Mm)` 发送 Command 4 请求
4. 服务器响应 Command 4，包含当前持仓数据
5. 调用处理器 `D` 解析订单，添加到 `ef[]` 数组

**Rust 实现**:
在 `src/client.rs` 中，收到 Command 3 后：
```rust
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
```

## 关键发现总结

1. **Command 3 不包含订单** - 只有账户信息、品种、报价
2. **Command 4 才是当前持仓数据源** - **需要客户端主动请求！**
   - 收到 Command 3 后立即发送 Command 4 请求 (mt4.en.js line 1181 → 1216)
   - 请求无参数，响应初始化 `ef[]` 数组
   - **这是请求-响应模式，而非服务器主动推送！**
3. **Command 5 用于历史订单** - 客户端主动请求
4. **Command 10 用于实时更新** - 服务器推送，维护 `ef[]` 和 `history[]` 的一致性

## 参考资料

- mt4.en.js line 1123: 命令码映射定义
- mt4.en.js line 1180: Command 3 处理函数
- mt4.en.js line 1181: Command 3 处理后调用 `C.F.$().lf()` 触发 Command 4 请求
- mt4.en.js line 1204: Command 4 处理函数 D (初始化 ef[])
- mt4.en.js line 1205: Command 5/10 处理函数
- mt4.en.js line 1216: **B.lf() 函数 - 发送 Command 4 请求的关键代码**
- mt4.en.js line 1296: ef[] 数组和 Oo() 函数定义
- mt4.en.js line 1103: Sr() / yC() 解析函数

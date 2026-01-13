# OrderUpdate 解析方式修改说明

## 修改日期
2026-01-07

## 修改原因
将 Rust 实现的订单解析方式改为与 JS 原始实现完全一致，采用简单的固定步长分割方式。

## 修改内容

### 1. `OrderUpdate::from_bytes` 方法
- **之前**: 自动识别 185/370 字节，特殊处理 Close By 对冲平仓
- **现在**: 固定按 185 字节解析，添加 `offset` 参数支持从指定位置解析

```rust
// 新签名
pub fn from_bytes(data: &[u8], offset: usize) -> Option<Self>
```

### 2. `OrderUpdate::parse_all` 方法
- **之前**: 循环尝试解析，根据实际大小(185/370)动态跳过
- **现在**: 简单计算 `count = data.len() / 185`，然后按固定步长循环

```rust
// 新实现（对应 JS）
let count = data.len() / 185;
for i in 0..count {
    let offset = i * 185;
    if let Some(update) = Self::from_bytes(data, offset) {
        results.push(update);
    }
}
```

对应的 JS 代码：
```javascript
q.yC=function(g){
    if(!g)return[];
    for(var k=[],f,c=Math.floor(g.byteLength/185),h=0;h<c;h++)
        (f=J(g,185*h))&&k.push(f);
    return k
};
```

### 3. Close By 对冲平仓处理变化

#### 之前的方式 (Rust 特有优化)
```rust
// 370 字节数据包 → 1 个 OrderUpdate
OrderUpdate {
    order: Order { ticket: 123, ... },
    related_order: Some(Order { ticket: 456, ... }),
    raw_size: 370,
}
```

#### 现在的方式 (与 JS 一致)
```rust
// 370 字节数据包 → 2 个 OrderUpdate
[
    OrderUpdate {
        order: Order { ticket: 123, ... },
        related_order: None,
        raw_size: 185,
    },
    OrderUpdate {
        order: Order { ticket: 456, ... },
        related_order: None,
        raw_size: 185,
    }
]
```

### 4. 字段变化
- `raw_size`: 始终为 185（之前可能是 185/370）
- `related_order`: 始终为 `None`（保留字段以维持 API 兼容性）

### 5. 方法变化
- `is_close_by()`: 现在总是返回 `false`

## 优势

1. **与 JS 实现完全一致**: 避免 Rust 和 JS 端行为差异
2. **代码更简单**: 移除了复杂的 370 字节特殊处理逻辑
3. **更容易维护**: 行为可预测，按固定规则分割
4. **API 兼容**: 保留了 `related_order` 字段，不影响现有代码

## 注意事项

1. **Close By 识别**: 业务层需要自行识别哪两个 OrderUpdate 属于同一个对冲平仓操作
2. **数据包大小**: 只处理 185 字节对齐的数据，不完整的尾部数据会被丢弃
3. **兼容性**: `related_order` 字段保留但始终为 `None`，确保向后兼容

## 测试
- ✅ 编译通过
- ✅ 单元测试通过
- ✅ 与 JS 行为一致

//! MT4 错误测试案例 - 测试各种会失败的订单
//!
//! 用法:
//! ```bash
//! cargo run --example error_test -- <login> <password> <server>
//! ```

use mt4_client::{LoginCredentials, Mt4Client, Mt4Event};
use std::env;
use std::time::Duration;
use tokio::time::timeout;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("mt4_client=info,error_test=info")),
        )
        .init();

    // 解析命令行参数
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!("用法: {} <login> <password> <server>", args[0]);
        std::process::exit(1);
    }

    let credentials = LoginCredentials {
        login: args[1].clone(),
        password: args[2].clone(),
        server: args[3].clone(),
    };

    println!("==================================================");
    println!("MT4 错误测试");
    println!("==================================================");

    let mut client = Mt4Client::new();
    client.connect(&credentials).await?;

    // 等待认证
    let auth_ok = timeout(Duration::from_secs(10), async {
        while let Some(event) = client.next_event().await {
            match event {
                Mt4Event::Authenticated => return true,
                Mt4Event::AuthFailed(_) => return false,
                _ => {}
            }
        }
        false
    }).await.unwrap_or(false);

    if !auth_ok {
        eprintln!("认证失败!");
        return Ok(());
    }
    println!("[OK] 认证成功\n");

    // 等待初始化
    tokio::time::sleep(Duration::from_secs(2)).await;

    // ==================== 测试1: 无效品种 ====================
    println!("==================================================");
    println!("[TEST 1] 无效品种: INVALIDPAIR");
    println!("==================================================");
    client.buy("INVALIDPAIR", 0.01, None, None).await?;
    wait_for_result(&mut client).await;

    tokio::time::sleep(Duration::from_secs(1)).await;

    // ==================== 测试2: 手数过大 (资金不足) ====================
    println!("\n==================================================");
    println!("[TEST 2] 手数过大: EURUSD 100手 (资金不足)");
    println!("==================================================");
    client.buy("EURUSD", 100.0, None, None).await?;
    wait_for_result(&mut client).await;

    tokio::time::sleep(Duration::from_secs(1)).await;

    // ==================== 测试3: 手数过小 ====================
    println!("\n==================================================");
    println!("[TEST 3] 手数过小: EURUSD 0.001手");
    println!("==================================================");
    client.buy("EURUSD", 0.001, None, None).await?;
    wait_for_result(&mut client).await;

    tokio::time::sleep(Duration::from_secs(1)).await;

    // ==================== 测试4: 无效止损 (太近) ====================
    println!("\n==================================================");
    println!("[TEST 4] 无效止损: 止损价格 = 0.0001 (太近)");
    println!("==================================================");
    client.buy("EURUSD", 0.01, Some(0.0001), None).await?;
    wait_for_result(&mut client).await;

    tokio::time::sleep(Duration::from_secs(1)).await;

    // ==================== 测试5: 平仓无效订单号 ====================
    println!("\n==================================================");
    println!("[TEST 5] 平仓无效订单: ticket=999999999");
    println!("==================================================");
    client.close_order(999999999, "EURUSD", 0.01).await?;
    wait_for_result(&mut client).await;

    tokio::time::sleep(Duration::from_secs(1)).await;

    // ==================== 测试6: 正常订单 (对比) ====================
    println!("\n==================================================");
    println!("[TEST 6] 正常订单: EURUSD 0.01手 (应该成功)");
    println!("==================================================");
    client.buy("EURUSD", 0.01, None, None).await?;
    wait_for_result(&mut client).await;

    println!("\n==================================================");
    println!("测试完成!");
    println!("==================================================");

    client.disconnect().await;
    Ok(())
}

async fn wait_for_result(client: &mut Mt4Client) {
    match timeout(Duration::from_secs(10), async {
        while let Some(event) = client.next_event().await {
            match event {
                Mt4Event::OrderUpdate(update) => {
                    println!("[ORDER] ✓ 订单成功!");
                    println!("        订单号: {}", update.order.ticket);
                    println!("        品种: {}", update.order.symbol);
                    println!("        类型: {:?}", update.order.order_type);
                    println!("        开仓价: {:.5}", update.order.open_price);
                    return Some("order_update");
                }
                Mt4Event::TradeSuccess { request_id, .. } => {
                    println!("[SUCCESS] ✓ 交易成功! 请求ID: {}", request_id);
                    return Some("success");
                }
                Mt4Event::TradeFailed { code, message } => {
                    println!("[FAILED] ✗ 交易失败!");
                    println!("         错误码: {}", code);
                    println!("         错误信息: {}", message);
                    return Some("failed");
                }
                _ => {}
            }
        }
        None
    }).await {
        Ok(Some(_)) => println!(""),
        Ok(None) => println!("[TIMEOUT] 等待结果超时\n"),
        Err(_) => println!("[TIMEOUT] 等待超时\n"),
    }
}

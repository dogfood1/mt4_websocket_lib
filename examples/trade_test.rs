//! MT4 交易测试案例
//!
//! 用法:
//! ```bash
//! cargo run --example trade_test -- <login> <password> <server>
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
                .unwrap_or_else(|_| EnvFilter::new("mt4_client=info,trade_test=info")),
        )
        .init();

    // 解析命令行参数
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!("用法: {} <login> <password> <server>", args[0]);
        eprintln!("示例: {} 31313724 password ICMarketsSC-Demo03", args[0]);
        std::process::exit(1);
    }

    let credentials = LoginCredentials {
        login: args[1].clone(),
        password: args[2].clone(),
        server: args[3].clone(),
    };

    println!("==================================================");
    println!("MT4 Rust Client 测试");
    println!("==================================================");
    println!("账号: {}", credentials.login);
    println!("服务器: {}", credentials.server);
    println!("==================================================");

    // 创建客户端
    let mut client = Mt4Client::new();

    // 连接
    println!("\n[1] 正在连接...");
    client.connect(&credentials).await?;
    println!("[1] 连接成功，等待认证...");

    // 等待认证
    let auth_result = timeout(Duration::from_secs(10), async {
        while let Some(event) = client.next_event().await {
            match event {
                Mt4Event::Authenticated => {
                    return Ok(());
                }
                Mt4Event::AuthFailed(code) => {
                    return Err(format!("认证失败，错误码: {}", code));
                }
                Mt4Event::Error(e) => {
                    return Err(format!("连接错误: {}", e));
                }
                _ => {}
            }
        }
        Err("连接断开".to_string())
    })
    .await;

    match auth_result {
        Ok(Ok(())) => println!("[2] *** 认证成功! ***"),
        Ok(Err(e)) => {
            eprintln!("[2] 认证失败: {}", e);
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("[2] 认证超时");
            std::process::exit(1);
        }
    }

    // 请求账户信息
    println!("\n[3] 请求账户信息...");
    client.request_account_info().await?;

    // 等待 2 秒获取初始数据
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 下单测试
    println!("\n[4] 下单测试: 买入 EURUSD 0.01 手...");
    client.buy("EURUSD", 0.01, None, None).await?;

    // 持续接收事件 (运行60秒)
    println!("\n[5] 持续监听事件 (60秒)...");
    println!("    按 Ctrl+C 退出\n");

    let start = std::time::Instant::now();
    let run_duration = Duration::from_secs(60);
    let mut last_ping = std::time::Instant::now();

    while start.elapsed() < run_duration {
        // 每30秒发送心跳
        if last_ping.elapsed() >= Duration::from_secs(30) {
            println!("[PING] 发送心跳...");
            client.ping().await?;
            last_ping = std::time::Instant::now();
        }

        match timeout(Duration::from_secs(5), client.next_event()).await {
            Ok(Some(event)) => {
                match event {
                    Mt4Event::OrderUpdate(update) => {
                        println!("\n[ORDER] ========================================");
                        println!("[ORDER] 订单号: {}", update.order.ticket);
                        println!("[ORDER] 品种: {}", update.order.symbol);
                        println!("[ORDER] 类型: {:?}", update.order.order_type);
                        println!("[ORDER] 手数: {:.2} 手", update.order.volume);
                        println!("[ORDER] 开仓价: {:.5}", update.order.open_price);
                        if update.order.sl > 0.0 {
                            println!("[ORDER] 止损: {:.5}", update.order.sl);
                        }
                        if update.order.tp > 0.0 {
                            println!("[ORDER] 止盈: {:.5}", update.order.tp);
                        }
                        if update.order.profit != 0.0 {
                            println!("[ORDER] 盈亏: {:.2}", update.order.profit);
                        }
                        if !update.order.comment.is_empty() {
                            println!("[ORDER] 注释: {}", update.order.comment);
                        }
                        println!("[ORDER] ========================================\n");
                    }
                    Mt4Event::TradeSuccess { request_id, status } => {
                        println!("[TRADE] *** 交易成功! 请求ID: {}, 状态: {} ***", request_id, status);
                    }
                    Mt4Event::TradeFailed { code, message } => {
                        println!("[TRADE] 交易失败: {} (代码: {})", message, code);
                    }
                    Mt4Event::Pong => {
                        println!("[PONG] 心跳响应");
                    }
                    Mt4Event::Disconnected => {
                        println!("[WARN] 连接断开!");
                        break;
                    }
                    Mt4Event::Error(e) => {
                        println!("[ERROR] {}", e);
                    }
                    Mt4Event::RawMessage { command, error_code, data } => {
                        println!("[RAW] 命令: {}, 错误: {}, 数据: {} 字节", command, error_code, data.len());
                    }
                    _ => {}
                }
            }
            Ok(None) => {
                println!("[WARN] 事件通道关闭");
                break;
            }
            Err(_) => {
                // 超时，继续等待
            }
        }
    }

    // 断开连接
    println!("\n[6] 断开连接...");
    client.disconnect().await;

    println!("\n==================================================");
    println!("测试完成!");
    println!("==================================================");

    Ok(())
}

//! MT4 交易测试案例
//!
//! 用法:
//! ```bash
//! cargo run --example trade_test -- <login> <password> <server>
//! ```

use mt4_client::{LoginCredentials, Mt4Client, Mt4Event};
use std::env;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::time::Duration;
use tokio::time::timeout;
use tracing_subscriber::EnvFilter;

const ORDER_LOG_FILE: &str = "orders.log";

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

    // 清空并创建订单日志文件
    let mut order_log = File::create(ORDER_LOG_FILE)?;
    writeln!(order_log, "# MT4 Order Log - {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))?;
    writeln!(order_log, "# Format: timestamp|notify_type|ticket|symbol|type|volume|open_price|close_price|sl|tp|profit|commission|swap|open_time|close_time|comment")?;
    writeln!(order_log, "#")?;
    drop(order_log);
    println!("订单日志: {}", ORDER_LOG_FILE);

    // 打印CSV表头到控制台
    println!("\n=== 订单实时监控 (CSV格式) ===");
    println!("时间,通知类型,订单号,品种,类型,手数,开仓价,平仓价,止损,止盈,盈亏,佣金,隔夜利息,开仓时间,平仓时间,注释");

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

    // 不自动获取历史订单，只监听实时订单更新
    // 如果需要获取历史订单，取消注释以下代码：
    // println!("\n[4] 请求订单历史（最近7天）...");
    // let now = std::time::SystemTime::now()
    //     .duration_since(std::time::UNIX_EPOCH)
    //     .unwrap()
    //     .as_secs() as i32;
    // let seven_days_ago = now - 7 * 24 * 3600;
    // client.request_order_history_range(seven_days_ago, now).await?;

    println!("\n[4] 跳过历史订单获取，只监听实时更新...");
    tokio::time::sleep(Duration::from_secs(1)).await;

    // // 下单测试
    // println!("\n[5] 下单测试: 买入 EURUSD 0.01 手...");
    // client.buy("EURUSD", 0.01, None, None).await?;

    // 持续接收事件 (无限循环，按 Ctrl+C 退出)
    println!("\n[6] 持续监听事件...");
    println!("    按 Ctrl+C 退出\n");

    let mut last_ping = std::time::Instant::now();

    loop {
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
                        let is_close_by = update.is_close_by();

                        // 根据 notify_type 判断状态 (基于 mt4.en.js 中的 T={su:0,Fw:1,eG:2,Iu:3})
                        // 0 = 新订单(New), 1 = 已平仓(Close), 2 = 订单修改(Modify), 3 = 账户更新
                        let status = if is_close_by {
                            "对冲平仓 (Close By)"
                        } else {
                            match update.notify_type {
                                0 => "新订单",
                                1 => "已平仓",
                                2 => "订单修改",
                                3 => "账户更新",
                                _ => "未知状态",
                            }
                        };

                        // 打印主订单（CSV格式）
                        // 使用 get_actual_close_price() 获取正确的平仓价格
                        let order = &update.order;
                        let actual_close_price = update.get_actual_close_price();
                        println!(
                            "{},{},{},{},{:?},{:.2},{:.5},{:.5},{:.5},{:.5},{:.2},{:.2},{:.2},{},{},{}",
                            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                            status,
                            order.ticket,
                            order.symbol,
                            order.order_type,
                            order.volume,
                            order.open_price,
                            actual_close_price,
                            order.sl,
                            order.tp,
                            order.profit,
                            order.commission,
                            order.swap,
                            order.open_time,
                            order.close_time,
                            order.comment.replace(',', ";")
                        );

                        // 写入订单日志
                        if let Ok(mut log_file) = OpenOptions::new().append(true).open(ORDER_LOG_FILE) {
                            let order = &update.order;
                            let actual_close_price = update.get_actual_close_price();
                            let _ = writeln!(
                                log_file,
                                "{}|{}|{}|{}|{:?}|{:.2}|{:.5}|{:.5}|{:.5}|{:.5}|{:.2}|{:.2}|{:.2}|{}|{}|{}",
                                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                                update.notify_type,
                                order.ticket,
                                order.symbol,
                                order.order_type,
                                order.volume,
                                order.open_price,
                                actual_close_price,
                                order.sl,
                                order.tp,
                                order.profit,
                                order.commission,
                                order.swap,
                                order.open_time,
                                order.close_time,
                                order.comment.replace('|', "_")
                            );
                        }

                        // 显示关联订单 (Close By) - CSV格式
                        if let Some(ref related) = update.related_order {
                            println!(
                                "{},{},{},{},{:?},{:.2},{:.5},{:.5},{:.5},{:.5},{:.2},{:.2},{:.2},{},{},{}",
                                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                                "对冲单",
                                related.ticket,
                                related.symbol,
                                related.order_type,
                                related.volume,
                                related.open_price,
                                related.close_price,
                                related.sl,
                                related.tp,
                                related.profit,
                                related.commission,
                                related.swap,
                                related.open_time,
                                related.close_time,
                                related.comment.replace(',', ";")
                            );

                            // 写入关联订单日志
                            if let Ok(mut log_file) = OpenOptions::new().append(true).open(ORDER_LOG_FILE) {
                                let _ = writeln!(
                                    log_file,
                                    "{}|{}|{}|{}|{:?}|{:.2}|{:.5}|{:.5}|{:.5}|{:.5}|{:.2}|{:.2}|{:.2}|{}|{}|{}",
                                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                                    update.notify_type,
                                    related.ticket,
                                    related.symbol,
                                    related.order_type,
                                    related.volume,
                                    related.open_price,
                                    related.close_price,
                                    related.sl,
                                    related.tp,
                                    related.profit,
                                    related.commission,
                                    related.swap,
                                    related.open_time,
                                    related.close_time,
                                    related.comment.replace('|', "_")
                                );
                            }
                        }
                    }
                    Mt4Event::AccountInfo(account) => {
                        println!("\n[ACCOUNT] ========================================");
                        println!("[ACCOUNT] 账号: {}", account.login);
                        println!("[ACCOUNT] 杠杆: 1:{}", account.leverage);
                        println!("[ACCOUNT] ----------------------------------------");
                        println!("[ACCOUNT] 余额: {:.2}", account.balance);
                        println!("[ACCOUNT] 净值: {:.2}", account.equity);
                        println!("[ACCOUNT] 已用保证金: {:.2}", account.margin);
                        println!("[ACCOUNT] 可用保证金: {:.2}", account.free_margin);
                        println!("[ACCOUNT] ----------------------------------------");
                        if !account.currency.is_empty() {
                            println!("[ACCOUNT] 货币: {}", account.currency);
                        }
                        if !account.name.is_empty() {
                            println!("[ACCOUNT] 名称: {}", account.name);
                        }
                        if !account.server.is_empty() {
                            println!("[ACCOUNT] 服务器: {}", account.server);
                        }
                        if !account.company.is_empty() {
                            println!("[ACCOUNT] 公司: {}", account.company);
                        }
                        println!("[ACCOUNT] ========================================\n");
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
    println!("\n[7] 断开连接...");
    client.disconnect().await;

    println!("\n==================================================");
    println!("测试完成!");
    println!("==================================================");

    Ok(())
}

use tokio::time::{self, Duration};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{info, error, warn, debug};

pub mod registry {
    tonic::include_proto!("registry");
}

use registry::{
    registry_service_client::RegistryServiceClient,
    ConnectionMessage, ConnectionRegister, Heartbeat,
    connection_message,
};

use crate::config::BotCfg;
use crate::database::BotDatabase;
use std::sync::Arc;
use arc_swap::ArcSwap;

/// 智能检测协议并构建连接 URL
fn build_gateway_url(address: &str) -> String {
    if address.starts_with("http://") || address.starts_with("https://") {
        // 如果地址已经包含协议，直接使用
        address.to_string()
    } else if address.contains("localhost") || address.starts_with("127.") || address.starts_with("192.168.") || address.starts_with("10.") {
        // 本地地址默认使用 HTTP
        format!("http://{}", address)
    } else {
        // 外部地址默认使用 HTTPS (适合 CF Tunnel 等)
        format!("https://{}", address)
    }
}

/// 启动反向连接模式客户端
pub async fn start_gateway_client(
    db: Arc<BotDatabase>,
    cfg: Arc<ArcSwap<BotCfg>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = cfg.load();
    let gateway_address = config.gateway_address.as_ref()
        .ok_or("Gateway address not configured")?;
    let api_key = config.gateway_api_key.as_ref()
        .ok_or("API key not configured")?;
    
    let gateway_url = build_gateway_url(gateway_address);
    info!("Connecting to gRPC gateway at: {} (resolved to: {})", gateway_address, gateway_url);
    
    let mut client = RegistryServiceClient::connect(gateway_url).await?;
    let (tx, rx) = tokio::sync::mpsc::channel(100);

    // 第一个消息发送注册
    tx.send(ConnectionMessage {
        message_type: Some(connection_message::MessageType::Register(ConnectionRegister {
            api_key: api_key.clone(),
            services: vec!["LicenseManagementService".to_string()],
            connection_id: "".to_string(),
        })),
    }).await?;

    info!("Sent registration message to gateway");

    // 启动心跳任务
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let _ = tx_clone.send(ConnectionMessage {
                message_type: Some(connection_message::MessageType::Heartbeat(Heartbeat {
                    timestamp: chrono::Utc::now().timestamp(),
                    connection_id: "conn-id".to_string(),
                })),
            }).await;
            debug!("Sent heartbeat to gateway");
        }
    });

    // 建立连接并处理响应
    let response = client.establish_connection(ReceiverStream::new(rx)).await?;
    let mut inbound = response.into_inner();

    info!("Gateway connection established, listening for messages");

    while let Some(message) = inbound.message().await? {
        // 处理来自网关的消息
        if let Some(message_type) = message.message_type {
            match message_type {
                connection_message::MessageType::Request(forward_req) => {
                    info!("Received ForwardRequest: {}", forward_req.method_path);
                    
                    let db_conn = db.inner();
                    let current_cfg = cfg.load();
                    
                    // 调用 grpc_handlers 处理请求
                    match crate::grpc_handlers::handle_grpc_request(&forward_req, db_conn, &current_cfg).await {
                        Ok(response_payload) => {
                            info!("Handler returned {} bytes of response data", response_payload.len());
                            debug!("Response payload bytes: {:?}", response_payload);
                            
                            // 发送响应回网关
                            let response_msg = ConnectionMessage {
                                message_type: Some(connection_message::MessageType::Response(
                                    registry::ForwardResponse {
                                        request_id: forward_req.request_id.clone(),
                                        status_code: 200,
                                        headers: std::collections::HashMap::new(),
                                        payload: response_payload.clone(),
                                        error_message: String::new(),
                                    }
                                )),
                            };
                            
                            debug!("ForwardResponse structure: {:#?}", response_msg);
                            info!("Sending response back to gateway for request {}", forward_req.request_id);
                            
                            if let Err(e) = tx.send(response_msg).await {
                                error!("Failed to send response: {}", e);
                            } else {
                                info!("Successfully sent response to gateway for request {}", forward_req.request_id);
                            }
                        },
                        Err(e) => {
                            error!("Failed to handle gRPC request: {}", e);
                            
                            // 发送错误响应
                            let error_response = ConnectionMessage {
                                message_type: Some(connection_message::MessageType::Response(
                                    registry::ForwardResponse {
                                        request_id: forward_req.request_id.clone(),
                                        status_code: 500,
                                        headers: std::collections::HashMap::new(),
                                        payload: Vec::new(),
                                        error_message: e.to_string(),
                                    }
                                )),
                            };
                            
                            if let Err(e) = tx.send(error_response).await {
                                error!("Failed to send error response: {}", e);
                            }
                        }
                    }
                },
                connection_message::MessageType::Response(response) => {
                    info!("Received Response: status {}", response.status_code);
                },
                _ => {
                    debug!("Received other message type");
                }
            }
        }
    }

    Ok(())
}

/// 带自动重连的网关客户端
pub async fn start_gateway_client_with_retry(
    db: Arc<BotDatabase>,
    cfg: Arc<ArcSwap<BotCfg>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut retry_count = 0;
    let max_retries = 10;
    let mut backoff_duration = Duration::from_secs(1);

    loop {
        match start_gateway_client(db.clone(), cfg.clone()).await {
            Ok(_) => {
                info!("Gateway connection established successfully");
                break;
            },
            Err(e) => {
                retry_count += 1;
                if retry_count >= max_retries {
                    error!("Failed to connect to gateway after {} retries: {}", max_retries, e);
                    return Err(format!("Failed to connect to gateway after {} retries: {}", max_retries, e).into());
                }

                warn!("Gateway connection failed (attempt {}): {}. Retrying in {:?}...",
                    retry_count, e, backoff_duration);
                
                tokio::time::sleep(backoff_duration).await;
                
                // 指数退避，最大60秒
                backoff_duration = std::cmp::min(backoff_duration * 2, Duration::from_secs(60));
            }
        }
    }

    Ok(())
}
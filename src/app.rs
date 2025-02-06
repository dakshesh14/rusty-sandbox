use axum::Router;

use crate::{apis::get_routes, constants::Settings};

pub async fn run_server() {
    let routes: Router = get_routes();

    let tcp_listener = tokio::net::TcpListener::bind(Settings::from_env().app_host)
        .await
        .unwrap();
    println!("Listening on: {}", tcp_listener.local_addr().unwrap());
    axum::serve(tcp_listener, routes).await.unwrap();
}

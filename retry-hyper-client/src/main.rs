use std::time::Duration;

use hyper::{Client, Request};
use hyper_rustls::HttpsConnectorBuilder;
use tower::{retry::RetryLayer, Layer, Service};

mod backoff;
use backoff::Backoff;

#[tokio::main]
async fn main() {
    // Rust-native HTTPS connector for hyper
    let https_connector = HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_only()
        .enable_http1()
        .enable_http2()
        .build();

    let policy = Backoff::default().with_max_delay(Duration::from_secs(2)).with_jitter(Duration::from_millis(10));
    let retry_layer = RetryLayer::new(policy);

    let mut client = retry_layer.layer(Client::builder().build::<_, String>(https_connector));

    let request = Request::builder()
        .uri("https://google.com/asdfasdf")
        .body(Default::default())
        .expect("failed to create request");

    let res = client.call(request).await.expect("failed to call");
    dbg!(res);
}

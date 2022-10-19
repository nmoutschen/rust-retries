use std::{future::Future, pin::Pin, time::Duration};

use hyper::{Body, Client, Error, Request, Response};
use hyper_rustls::HttpsConnectorBuilder;
use tokio::time::sleep;
use tower::{
    retry::{Policy, RetryLayer},
    Layer, Service,
};

#[tokio::main]
async fn main() {
    // Rust-native HTTPS connector for hyper
    let https_connector = HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_only()
        .enable_http1()
        .enable_http2()
        .build();

    let retry_layer = RetryLayer::new(Backoff::default().with_max_delay(Duration::from_secs(2)));

    let mut client = retry_layer.layer(Client::builder().build::<_, String>(https_connector));

    let request = Request::builder()
        .uri("https://google.com/asdfasdf")
        .body(Default::default())
        .expect("failed to create request");

    let res = client.call(request).await.expect("failed to call");
    dbg!(res);
}

#[derive(Clone)]
struct Backoff {
    attempts: usize,
    delay: Duration,
    multiplier: f64,
    max_delay: Option<Duration>,
}

impl Backoff {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn with_attempts(self, attempts: usize) -> Self {
        Self { attempts, ..self }
    }

    #[allow(dead_code)]
    pub fn with_delay(self, delay: Duration) -> Self {
        Self { delay, ..self }
    }

    #[allow(dead_code)]
    pub fn with_multiplier(self, multiplier: f64) -> Self {
        Self { multiplier, ..self }
    }

    #[allow(dead_code)]
    pub fn with_max_delay(self, max_delay: Duration) -> Self {
        Self {
            max_delay: Some(max_delay),
            ..self
        }
    }

    pub async fn next(&self) -> Self {
        sleep(self.delay).await;

        let delay = self.delay.mul_f64(self.multiplier);
        let delay = if let Some(max_delay) = self.max_delay {
            if delay > max_delay {
                max_delay
            } else {
                delay
            }
        } else {
            delay
        };
        Self {
            attempts: self.attempts - 1,
            delay,
            multiplier: self.multiplier,
            max_delay: self.max_delay,
        }
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            attempts: 10,
            delay: Duration::from_millis(100),
            multiplier: 2.0,
            max_delay: None,
        }
    }
}

// TODO: exponential backoff
impl Policy<Request<String>, Response<Body>, Error> for Backoff {
    type Future = Pin<Box<dyn Future<Output = Self>>>;

    fn retry(
        &self,
        req: &Request<String>,
        result: Result<&Response<Body>, &Error>,
    ) -> Option<Self::Future> {
        // Used all the attempts, stopping now
        if self.attempts <= 0 {
            return None;
        }

        println!(
            "calling {} ({} attempts left, {}ms delay)",
            req.uri(),
            self.attempts,
            self.delay.as_millis()
        );

        match result {
            Ok(response) => {
                let status = response.status();
                let new_self = self.clone();
                // Retry on 4xx and 5xx
                if status.is_server_error() || status.is_client_error() {
                    Some(Box::pin(async move { new_self.next().await }))
                } else {
                    None
                }
            }
            Err(_err) => {
                let new_self = self.clone();
                Some(Box::pin(async move { new_self.next().await }))
            }
        }
    }

    fn clone_request(&self, req: &Request<String>) -> Option<Request<String>> {
        // `Request` can't be cloned
        let mut new_req = Request::builder()
            .uri(req.uri())
            .method(req.method())
            .version(req.version());
        for (name, value) in req.headers() {
            new_req = new_req.header(name, value);
        }
        let body = req.body().clone();
        let new_req = new_req.body(body).expect("failed to build request");

        Some(new_req)
    }
}

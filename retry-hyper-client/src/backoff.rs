use std::{future::Future, pin::Pin, time::Duration};

use hyper::{Body, Error, Request, Response};
use rand::distributions::Uniform;
#[cfg(feature = "rand")]
use rand::Rng;
use tokio::time::sleep;
use tower::retry::Policy;

/// Exponential backoff with maximum delay
#[derive(Clone)]
pub struct Backoff {
    /// Maximum number of attempts before failing
    attempts: usize,
    /// Initial delay
    delay: Duration,
    /// Multiplier for each delay
    multiplier: f64,
    /// Maximum delay
    max_delay: Option<Duration>,
    /// Jitter to add on calls
    ///
    /// If this contains some value, this will add a random jitter between `-jitter` and `+jitter`.
    #[cfg(feature = "rand")]
    jitter: Option<Jitter>,
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

    #[cfg(feature = "rand")]
    #[allow(dead_code)]
    pub fn with_jitter<J: Into<Jitter>>(self, jitter: J) -> Self {
        Self {
            jitter: Some(jitter.into()),
            ..self
        }
    }

    pub async fn next(&self) -> Self {
        let delay = self.delay;
        #[cfg(feature = "rand")]
        let delay = match self.jitter {
            Some(Jitter::Duration(jitter)) => {
                let mut rng = rand::thread_rng();
                let jitter = rng.sample(Uniform::new(Duration::new(0, 0), jitter));
                self.delay + jitter
            }
            Some(Jitter::Percentage(percentage)) => {
                let mut rng = rand::thread_rng();
                let jitter = rng.sample(Uniform::new(0.0, percentage));
                self.delay.mul_f64(1.0 + jitter)
            }
            None => delay,
        };

        println!("effective delay: {}ms", delay.as_millis());

        sleep(delay).await;

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
            jitter: self.jitter,
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
            jitter: None,
        }
    }
}

impl<T> Policy<Request<T>, Response<Body>, Error> for Backoff
where
    T: Clone,
{
    type Future = Pin<Box<dyn Future<Output = Self>>>;

    fn retry(
        &self,
        req: &Request<T>,
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

    fn clone_request(&self, req: &Request<T>) -> Option<Request<T>> {
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

#[derive(Clone, Debug, Copy)]
pub enum Jitter {
    /// Maximum jitter duration to add to delays between attempts
    Duration(Duration),
    /// Maximum percentage of jitter to add to delays between attempts
    Percentage(f64),
}

impl Into<Jitter> for f64 {
    fn into(self) -> Jitter {
        Jitter::Percentage(self)
    }
}

impl Into<Jitter> for Duration {
    fn into(self) -> Jitter {
        Jitter::Duration(self)
    }
}

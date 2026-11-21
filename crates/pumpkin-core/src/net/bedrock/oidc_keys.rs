//! The signing keys Bedrock login tokens are checked against.
//!
//! They come from Minecraft's OIDC discovery endpoint. A failed fetch is retried with backoff
//! instead of leaving the server with no keys until a restart, and a token signed with a key the
//! set doesn't have triggers a refetch, at most once per [`MIN_REFETCH_INTERVAL`], so a key
//! rotation doesn't lock players out.

use std::{sync::Arc, time::Duration};

use arc_swap::ArcSwapOption;
use pumpkin_auth::jwt::{Jwks, fetch_oidc_jwks};
use pumpkin_config::networking::bedrock::BedrockAuthenticationConfig;
use tokio::{sync::Mutex, time::Instant};
use tracing::{info, warn};

/// The issuer and key set of the last good fetch.
pub type OidcKeySet = (String, Jwks);

/// The shortest time between two fetches started by logins.
pub const MIN_REFETCH_INTERVAL: Duration = Duration::from_secs(60);
const RETRY_START: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(600);

pub struct OidcKeys {
    url: Option<String>,
    connect_timeout: u32,
    read_timeout: u32,
    min_refetch: Duration,
    retry_start: Duration,
    retry_max: Duration,
    keys: ArcSwapOption<OidcKeySet>,
    /// Held for the length of a fetch, so concurrent logins wait for one fetch.
    fetch: Mutex<FetchState>,
}

struct FetchState {
    /// The earliest a login may start another fetch.
    next_allowed: Option<Instant>,
    /// How long to wait after the next failure.
    retry: Duration,
}

impl OidcKeys {
    #[must_use]
    pub fn new(config: &BedrockAuthenticationConfig) -> Self {
        Self::with_intervals(config, MIN_REFETCH_INTERVAL, RETRY_START, RETRY_MAX)
    }

    fn with_intervals(
        config: &BedrockAuthenticationConfig,
        min_refetch: Duration,
        retry_start: Duration,
        retry_max: Duration,
    ) -> Self {
        Self {
            url: config.url.clone(),
            connect_timeout: config.connect_timeout,
            read_timeout: config.read_timeout,
            min_refetch,
            retry_start,
            retry_max,
            keys: ArcSwapOption::empty(),
            fetch: Mutex::new(FetchState {
                next_allowed: None,
                retry: retry_start,
            }),
        }
    }

    /// The keys from the last good fetch, without fetching.
    #[must_use]
    pub fn current(&self) -> Option<Arc<OidcKeySet>> {
        self.keys.load_full()
    }

    /// The keys to check a token signed with `kid` against. Fetches first when there are no keys
    /// yet or they lack `kid`, unless a fetch ran too recently, in which case the current keys
    /// (possibly none) are returned and the token fails as before.
    pub async fn for_kid(&self, kid: &str) -> Option<Arc<OidcKeySet>> {
        let usable = |keys: &Arc<OidcKeySet>| keys.1.keys.iter().any(|key| key.kid == kid);
        if let Some(keys) = self.current().filter(usable) {
            return Some(keys);
        }
        let mut state = self.fetch.lock().await;
        // Another login may have fetched while this one waited for the lock.
        if let Some(keys) = self.current().filter(usable) {
            return Some(keys);
        }
        if state.next_allowed.is_none_or(|at| Instant::now() >= at) {
            self.fetch_locked(&mut state).await;
        }
        self.current()
    }

    /// Fetches until a fetch succeeds, waiting longer after each failure. Returns at once if a
    /// login already fetched the keys.
    pub async fn fetch_until_ready(&self) {
        loop {
            let wait = {
                let mut state = self.fetch.lock().await;
                if self.current().is_some() || self.fetch_locked(&mut state).await {
                    return;
                }
                state
                    .next_allowed
                    .map_or(self.retry_start, |at| at - Instant::now())
            };
            tokio::time::sleep(wait).await;
        }
    }

    async fn fetch_locked(&self, state: &mut FetchState) -> bool {
        match fetch_oidc_jwks(self.url.as_deref(), self.connect_timeout, self.read_timeout).await {
            Ok(keys) => {
                info!("Fetched {} Bedrock OIDC keys", keys.1.keys.len());
                self.keys.store(Some(Arc::new(keys)));
                state.next_allowed = Some(Instant::now() + self.min_refetch);
                state.retry = self.retry_start;
                true
            }
            Err(error) => {
                warn!(
                    "Failed to fetch Bedrock OIDC keys, retrying in {}s: {error}",
                    state.retry.as_secs()
                );
                state.next_allowed = Some(Instant::now() + state.retry);
                state.retry = (state.retry * 2).min(self.retry_max);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Mutex as StdMutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use pumpkin_auth::{
        jwt::{OIDC_AUDIENCE, verify_oidc_token},
        p384::{
            ecdsa::{Signature, SigningKey, signature::Signer},
            pkcs8::EncodePublicKey,
        },
    };
    use serde_json::{Value, json};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    use super::*;

    const ISSUER: &str = "https://oidc.test";

    /// A local stand-in for the discovery, `OpenID` configuration and JWKS endpoints.
    struct KeyServer {
        url: String,
        keys: StdMutex<Vec<Value>>,
        failing: AtomicBool,
        key_fetches: AtomicUsize,
    }

    impl KeyServer {
        async fn start(keys: Vec<Value>, failing: bool) -> Arc<Self> {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let server = Arc::new(Self {
                url: format!("http://{}", listener.local_addr().unwrap()),
                keys: StdMutex::new(keys),
                failing: AtomicBool::new(failing),
                key_fetches: AtomicUsize::new(0),
            });
            let state = server.clone();
            tokio::spawn(async move {
                loop {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let state = state.clone();
                    tokio::spawn(async move {
                        let mut request = Vec::new();
                        let mut buf = [0; 1024];
                        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                            let n = stream.read(&mut buf).await.unwrap();
                            if n == 0 {
                                return;
                            }
                            request.extend_from_slice(&buf[..n]);
                        }
                        let request = String::from_utf8_lossy(&request);
                        let path = request.split_whitespace().nth(1).unwrap_or_default();
                        let (status, body) = state.respond(path);
                        let response = format!(
                            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                    });
                }
            });
            server
        }

        fn respond(&self, path: &str) -> (&'static str, String) {
            match path {
                "/discovery" => (
                    "200 OK",
                    json!({"result": {"serviceEnvironments": {"auth": {"prod": {"serviceUri": self.url}}}}})
                        .to_string(),
                ),
                "/.well-known/openid-configuration" => (
                    "200 OK",
                    json!({"issuer": ISSUER, "jwks_uri": format!("{}/keys", self.url)}).to_string(),
                ),
                "/keys" => {
                    self.key_fetches.fetch_add(1, Ordering::SeqCst);
                    if self.failing.load(Ordering::SeqCst) {
                        ("503 Service Unavailable", "unavailable".to_owned())
                    } else {
                        let keys = self.keys.lock().unwrap().clone();
                        ("200 OK", json!({ "keys": keys }).to_string())
                    }
                }
                _ => ("404 Not Found", String::new()),
            }
        }

        fn config(&self) -> BedrockAuthenticationConfig {
            BedrockAuthenticationConfig {
                url: Some(format!("{}/discovery", self.url)),
                ..BedrockAuthenticationConfig::default()
            }
        }
    }

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_slice(&[seed; 48]).unwrap()
    }

    fn jwk(kid: &str, key: &SigningKey) -> Value {
        // The DER public key ends with the uncompressed point: 0x04, X, Y.
        let der = key.verifying_key().to_public_key_der().unwrap();
        let point = &der.as_bytes()[der.as_bytes().len() - 97..];
        json!({
            "kty": "EC",
            "crv": "P-384",
            "alg": "ES384",
            "kid": kid,
            "x": URL_SAFE_NO_PAD.encode(&point[1..49]),
            "y": URL_SAFE_NO_PAD.encode(&point[49..]),
        })
    }

    fn token(kid: &str, key: &SigningKey) -> String {
        let exp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;
        let header = URL_SAFE_NO_PAD.encode(json!({"alg": "ES384", "kid": kid}).to_string());
        let payload = URL_SAFE_NO_PAD.encode(
            json!({"iss": ISSUER, "aud": OIDC_AUDIENCE, "exp": exp, "xid": "2535000000000001", "xname": "Bot"})
                .to_string(),
        );
        let signature: Signature = key.sign(format!("{header}.{payload}").as_bytes());
        format!(
            "{header}.{payload}.{}",
            URL_SAFE_NO_PAD.encode(signature.to_bytes())
        )
    }

    fn verifies(keys: Option<&Arc<OidcKeySet>>, token: &str) -> bool {
        keys.is_some_and(|keys| verify_oidc_token(token, &keys.0, &keys.1).is_ok())
    }

    fn short_intervals(config: &BedrockAuthenticationConfig) -> OidcKeys {
        OidcKeys::with_intervals(
            config,
            Duration::from_millis(300),
            Duration::from_millis(50),
            Duration::from_millis(200),
        )
    }

    #[tokio::test]
    async fn failed_first_fetch_is_retried_in_the_background() {
        let key = signing_key(1);
        let server = KeyServer::start(vec![jwk("a", &key)], true).await;
        let keys = Arc::new(short_intervals(&server.config()));
        let task = tokio::spawn({
            let keys = keys.clone();
            async move { keys.fetch_until_ready().await }
        });
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert!(keys.current().is_none(), "the failing fetch stored keys");
        server.failing.store(false, Ordering::SeqCst);
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("the retry loop gave up")
            .unwrap();
        assert!(server.key_fetches.load(Ordering::SeqCst) >= 2);
        assert!(verifies(
            keys.for_kid("a").await.as_ref(),
            &token("a", &key)
        ));
    }

    #[tokio::test]
    async fn login_after_a_failed_fetch_fetches_again() {
        let key = signing_key(1);
        let server = KeyServer::start(vec![jwk("a", &key)], true).await;
        let keys = short_intervals(&server.config());
        assert!(keys.for_kid("a").await.is_none());
        server.failing.store(false, Ordering::SeqCst);
        // Inside the backoff a login doesn't fetch.
        assert!(keys.for_kid("a").await.is_none());
        assert_eq!(server.key_fetches.load(Ordering::SeqCst), 1);
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(verifies(
            keys.for_kid("a").await.as_ref(),
            &token("a", &key)
        ));
        assert_eq!(server.key_fetches.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn unknown_kid_refetches_once_per_interval() {
        let (old_key, new_key) = (signing_key(1), signing_key(2));
        let server = KeyServer::start(vec![jwk("old", &old_key)], false).await;
        // The checks after each fetch must run within one refetch interval, so it is long enough
        // for a loaded CI machine.
        let keys = OidcKeys::with_intervals(
            &server.config(),
            Duration::from_millis(1500),
            Duration::from_millis(50),
            Duration::from_millis(200),
        );
        keys.fetch_until_ready().await;
        assert!(verifies(
            keys.for_kid("old").await.as_ref(),
            &token("old", &old_key)
        ));
        assert_eq!(server.key_fetches.load(Ordering::SeqCst), 1);

        server.keys.lock().unwrap().push(jwk("new", &new_key));
        // Too soon after the last fetch: the rotated key isn't picked up yet.
        assert!(!verifies(
            keys.for_kid("new").await.as_ref(),
            &token("new", &new_key)
        ));
        assert_eq!(server.key_fetches.load(Ordering::SeqCst), 1);

        tokio::time::sleep(Duration::from_millis(1550)).await;
        assert!(verifies(
            keys.for_kid("new").await.as_ref(),
            &token("new", &new_key)
        ));
        assert_eq!(server.key_fetches.load(Ordering::SeqCst), 2);
        // Known keys and an unknown one right after a refetch cost no further fetch.
        assert!(verifies(
            keys.for_kid("old").await.as_ref(),
            &token("old", &old_key)
        ));
        assert!(keys.for_kid("missing").await.is_some());
        assert_eq!(server.key_fetches.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn concurrent_logins_share_one_fetch() {
        let key = signing_key(1);
        let server = KeyServer::start(vec![jwk("a", &key)], false).await;
        let keys = Arc::new(short_intervals(&server.config()));
        // Spawn all of them before awaiting any.
        let logins: Vec<_> = (0..8)
            .map(|_| {
                let keys = keys.clone();
                tokio::spawn(async move { keys.for_kid("a").await })
            })
            .collect();
        for login in logins {
            assert!(login.await.unwrap().is_some());
        }
        assert_eq!(server.key_fetches.load(Ordering::SeqCst), 1);
    }
}

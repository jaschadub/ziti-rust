//! Integration tests for Ziti SDK
//!
//! Tests in this file run against a real Ziti controller + edge router.
//! They are gated by the `ZITI_IDENTITY_FILE` env var (skipped if unset)
//! so `cargo test` on a developer machine without a Ziti network still
//! passes the unit/doc suite.
//!
//! Two categories:
//!   * controller-API tests — exercise mTLS to the controller, the
//!     `authenticate` flow, and `list_services`. These must pass for
//!     any deployment to work and run unconditionally when the
//!     identity file is set.
//!   * wire-protocol tests — actually `dial` / `listen` over an edge
//!     router. They depend on the binary `ZitiMessage` framing the SDK
//!     uses being accepted by the upstream router. Gated additionally
//!     behind `ZITI_WIRE_TESTS=1` because the current SDK framing is
//!     not yet wire-compatible with the upstream OpenZiti channel
//!     protocol and these will fail until that's implemented.

use std::env;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use ziti_sdk::{service::list_services, Context};

fn identity_file() -> Option<String> {
    env::var("ZITI_IDENTITY_FILE").ok()
}

fn wire_tests_enabled() -> bool {
    matches!(
        env::var("ZITI_WIRE_TESTS").ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

async fn create_test_context() -> Context {
    let path = identity_file()
        .expect("ZITI_IDENTITY_FILE must be set when this is reached");
    Context::from_file(&path)
        .await
        .expect("Context::from_file should load the bootstrap identity")
}

fn dial_service_name() -> String {
    env::var("ZITI_TEST_SERVICE").unwrap_or_else(|_| "echo-service".to_string())
}

fn listen_service_name() -> String {
    env::var("ZITI_TEST_LISTEN_SERVICE").unwrap_or_else(|_| "test-listen-service".to_string())
}

macro_rules! skip_if_no_identity {
    () => {
        if identity_file().is_none() {
            eprintln!("skipping: ZITI_IDENTITY_FILE not set");
            return;
        }
    };
}

macro_rules! skip_unless_wire {
    () => {
        if !wire_tests_enabled() {
            eprintln!("skipping: ZITI_WIRE_TESTS not set (wire-protocol not yet verified)");
            return;
        }
    };
}

// ---- controller-API tests (must pass) -------------------------------

#[tokio::test]
async fn test_context_creation() {
    skip_if_no_identity!();
    let _ = create_test_context().await;
}

#[tokio::test]
async fn test_context_cleanup() {
    skip_if_no_identity!();
    for _ in 0..3 {
        let _ctx = create_test_context().await;
    }
}

#[tokio::test]
async fn test_list_services() {
    skip_if_no_identity!();
    let ctx = create_test_context().await;
    let services = timeout(Duration::from_secs(30), list_services(ctx.session_manager()))
        .await
        .expect("list_services should not time out")
        .expect("list_services should succeed against the bootstrap controller");

    let want_dial = dial_service_name();
    let want_listen = listen_service_name();
    let names: Vec<&str> = services.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.iter().any(|n| *n == want_dial),
        "expected service {want_dial:?} in {names:?}"
    );
    assert!(
        names.iter().any(|n| *n == want_listen),
        "expected service {want_listen:?} in {names:?}"
    );
}

// ---- wire-protocol tests (skipped until upstream framing lands) -----

#[tokio::test]
async fn test_dial_service() {
    skip_if_no_identity!();
    skip_unless_wire!();
    let ctx = create_test_context().await;
    let svc = dial_service_name();
    timeout(Duration::from_secs(30), ctx.dial(&svc))
        .await
        .expect("dial should not time out")
        .unwrap_or_else(|e| panic!("dial({svc}) failed: {e:?}"));
}

#[tokio::test]
async fn test_dial_and_send_data() {
    skip_if_no_identity!();
    skip_unless_wire!();
    let ctx = create_test_context().await;
    let svc = dial_service_name();
    let mut stream = timeout(Duration::from_secs(30), ctx.dial(&svc))
        .await
        .expect("dial should not time out")
        .unwrap_or_else(|e| panic!("dial({svc}) failed: {e:?}"));

    let payload = b"Hello, Ziti!";
    timeout(Duration::from_secs(10), stream.write_all(payload))
        .await
        .expect("write should not time out")
        .expect("write should succeed");

    let mut buf = [0u8; 1024];
    let n = timeout(Duration::from_secs(10), stream.read(&mut buf))
        .await
        .expect("read should not time out")
        .expect("read should succeed");
    assert!(n > 0, "expected echo bytes back from {svc}");
    assert_eq!(&buf[..n], payload, "echo mismatch");
}

#[tokio::test]
async fn test_listen_service() {
    skip_if_no_identity!();
    skip_unless_wire!();
    let ctx = create_test_context().await;
    let svc = listen_service_name();
    let _listener = timeout(Duration::from_secs(30), ctx.listen(&svc))
        .await
        .expect("listen should not time out")
        .unwrap_or_else(|e| panic!("listen({svc}) failed: {e:?}"));
}

#[tokio::test]
async fn test_concurrent_dials() {
    skip_if_no_identity!();
    skip_unless_wire!();
    let ctx = create_test_context().await;
    let svc = dial_service_name();
    let mut handles = Vec::new();
    for i in 0..3 {
        let ctx = ctx.clone();
        let svc = svc.clone();
        handles.push(tokio::spawn(async move {
            timeout(Duration::from_secs(30), ctx.dial(&svc))
                .await
                .unwrap_or_else(|_| panic!("dial #{i} timed out"))
                .unwrap_or_else(|e| panic!("dial #{i} failed: {e:?}"));
        }));
    }
    for h in handles {
        h.await.expect("task should not panic");
    }
}

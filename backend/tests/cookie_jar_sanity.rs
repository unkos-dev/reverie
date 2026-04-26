//! D0.11 — verify that `axum_extra::extract::cookie::CookieJar` returned as
//! part of a tuple response emits a `Set-Cookie` header for both `StatusCode`
//! and `Redirect` response-tails. The whole design-system backend sliver
//! (theme PATCH, OIDC callback cookie seed, FOUC-hash cookie write) depends
//! on this contract. Failing fast here is the whole point of D0.
//!
//! This test lives under `backend/tests/` so the routes are automatically
//! cfg-gated to `test` — they never exist in the production router.

use axum::{Router, http::StatusCode, response::Redirect, routing::get};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use axum_test::TestServer;

async fn cookie_ok(jar: CookieJar) -> (CookieJar, StatusCode) {
    (jar.add(Cookie::new("test_ok", "1")), StatusCode::OK)
}

async fn cookie_redirect(jar: CookieJar) -> (CookieJar, Redirect) {
    (
        jar.add(Cookie::new("test_redirect", "1")),
        Redirect::temporary("/"),
    )
}

fn router() -> Router {
    Router::new()
        .route("/_test/cookie-ok", get(cookie_ok))
        .route("/_test/cookie-redirect", get(cookie_redirect))
}

#[tokio::test]
async fn cookie_jar_tuple_with_status_emits_set_cookie() {
    let server = TestServer::new(router());
    let resp = server.get("/_test/cookie-ok").await;
    resp.assert_status_ok();
    let header = resp
        .headers()
        .get("set-cookie")
        .expect("set-cookie header missing")
        .to_str()
        .expect("set-cookie header not ascii");
    assert!(
        header.starts_with("test_ok=1"),
        "unexpected set-cookie value: {header}"
    );
}

#[tokio::test]
async fn cookie_jar_tuple_with_redirect_emits_set_cookie() {
    // expect_failure() asserts non-2xx; without it, axum-test panics on
    // the 307. The mock transport never auto-follows redirects regardless,
    // so the response headers are always the originating handler's.
    let server = TestServer::new(router());
    let resp = server.get("/_test/cookie-redirect").expect_failure().await;
    assert_eq!(resp.status_code(), StatusCode::TEMPORARY_REDIRECT);
    let header = resp
        .headers()
        .get("set-cookie")
        .expect("set-cookie header missing on redirect")
        .to_str()
        .expect("set-cookie header not ascii");
    assert!(
        header.starts_with("test_redirect=1"),
        "unexpected set-cookie value: {header}"
    );
}

//! Local HTTP access control. This is a pre-shared token, not an OAuth server.
use super::server::Result;
use axum::{
    extract::Request,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use subtle::ConstantTimeEq;

pub(super) fn auth_token() -> Result<Option<String>> {
    match std::env::var("CHIRRP_MCP_AUTH_TOKEN") {
        Ok(token)
            if (32..=256).contains(&token.len())
                && token.bytes().all(|c| c.is_ascii_alphanumeric() || b"-._~+/=".contains(&c)) =>
        {
            Ok(Some(token))
        }
        Err(std::env::VarError::NotPresent) => Ok(None),
        _ => Err("CHIRRP_MCP_AUTH_TOKEN must contain 32–256 ASCII bearer-token characters; unset it to disable authentication".into()),
    }
}

pub(super) fn authorized(headers: &HeaderMap, token: &str) -> bool {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let Some(value) = values.next().and_then(|value| value.to_str().ok()) else {
        return false;
    };
    if values.next().is_some() {
        return false;
    }
    let Some((scheme, credential)) = value.split_once(' ') else {
        return false;
    };
    scheme.eq_ignore_ascii_case("Bearer")
        && bool::from(credential.as_bytes().ct_eq(token.as_bytes()))
}

pub(super) async fn protect(request: Request, next: Next, token: Option<String>) -> Response {
    // The control endpoint always uses its own per-instance shutdown secret.
    let mut response = if request.uri().path() != "/_shutdown"
        && token
            .as_deref()
            .is_some_and(|token| !authorized(request.headers(), token))
    {
        (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer realm=\"chirrp-mcp\"")],
            "Bearer authentication required",
        )
            .into_response()
    } else if request.headers().get_all(header::HOST).iter().count() != 1
        || request.headers().get_all(header::ORIGIN).iter().count() > 1
    {
        StatusCode::FORBIDDEN.into_response()
    } else {
        next.run(request).await
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

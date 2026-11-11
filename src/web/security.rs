use super::*;

pub(super) async fn security_headers(
    cx: &Cx,
    body: Body,
    next: Next<'_>,
) -> topcoat::Result<Response> {
    let mut response = next.run(cx, body).await.into_response(cx)?;
    let is_api = topcoat::router::request::uri(cx)
        .path()
        .starts_with("/api/");
    if is_api && (response.status().is_client_error() || response.status().is_server_error()) {
        let status = response.status();
        let message = match status.as_u16() {
            400 => "invalid request",
            401 => "unauthorized",
            403 => "forbidden",
            404 => "not found",
            413 => "request too large",
            429 => "too many requests",
            _ => "request failed",
        };
        let json = serde_json::json!({"error": message}).to_string();
        *response.body_mut() = Body::from(json);
        response.headers_mut().remove(http::header::CONTENT_LENGTH);
        response.headers_mut().insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
    }
    let headers = response.headers_mut();
    headers.insert(
        http::header::X_CONTENT_TYPE_OPTIONS,
        http::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        http::header::X_FRAME_OPTIONS,
        http::HeaderValue::from_static("DENY"),
    );
    headers.insert(
        http::header::REFERRER_POLICY,
        http::HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(http::header::CONTENT_SECURITY_POLICY, http::HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; form-action 'self'; frame-ancestors 'none'; base-uri 'self'"));
    // Only asset responses set a cache policy themselves. Unknown asset paths
    // and every personalized response remain non-cacheable.
    if !headers.contains_key(http::header::CACHE_CONTROL) {
        headers.insert(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("no-store"),
        );
    }
    Ok(response)
}

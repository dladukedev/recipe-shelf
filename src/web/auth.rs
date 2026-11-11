use super::*;

pub(super) fn safe_cookie_jar(cx: &Cx) -> impl Cookies {
    cookies(cx)
        .default_path("/")
        .default_same_site(SameSite::Strict)
        .default_secure(
            std::env::var("COOKIE_SECURE")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
        )
}
pub(super) fn csrf_token(cx: &Cx) -> String {
    let jar = safe_cookie_jar(cx);
    if let Some(cookie) = jar.get(CSRF) {
        return cookie.value().to_owned();
    }
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let value = hex::encode(bytes);
    jar.add(
        Cookie::build((CSRF, value.clone()))
            .http_only(false)
            .build(),
    );
    value
}
pub(super) fn session_token(cx: &Cx) -> Option<String> {
    safe_cookie_jar(cx)
        .get(SESSION)
        .map(|c| c.value().to_owned())
}
#[memoize]
pub(super) async fn authenticated_user(cx: &Cx, token: &str) -> topcoat::Result<Option<User>> {
    let token = token.to_owned();
    db(cx, move |a| a.authenticate(&token))
        .await
        .map_err(server_error)
}

pub(super) async fn current_web_user(cx: &Cx) -> topcoat::Result<Option<User>> {
    match session_token(cx) {
        Some(token) => authenticated_user(cx, &token).await.clone(),
        None => Ok(None),
    }
}

pub(super) async fn web_user(cx: &Cx) -> topcoat::Result<User> {
    current_web_user(cx)
        .await?
        .ok_or_else(|| unauthorized().into())
}

pub(super) async fn require_web_user(cx: &Cx) -> topcoat::Result<User> {
    current_web_user(cx)
        .await?
        .ok_or_else(|| redirect(href!(crate::web::login::login_get).resolve(cx)).into())
}

pub(super) async fn require_admin(cx: &Cx) -> topcoat::Result<User> {
    ensure_admin(web_user(cx).await?)
}

pub(super) async fn require_web_admin(cx: &Cx) -> topcoat::Result<User> {
    ensure_admin(require_web_user(cx).await?)
}

fn ensure_admin(user: User) -> topcoat::Result<User> {
    if !user.is_admin {
        return Err(forbidden().into());
    }
    Ok(user)
}

pub(super) fn require_csrf(cx: &Cx, submitted: &str) -> topcoat::Result<()> {
    if !checked_csrf(cx, submitted) {
        return Err(forbidden().into());
    }
    Ok(())
}

pub(super) fn checked_csrf(cx: &Cx, submitted: &str) -> bool {
    let expected = safe_cookie_jar(cx).get(CSRF).map(|c| c.value().to_owned());
    let origin_ok = topcoat::router::request::headers(cx)
        .get("origin")
        .and_then(|h| h.to_str().ok())
        .map(|origin| {
            topcoat::router::request::headers(cx)
                .get("host")
                .and_then(|h| h.to_str().ok())
                .map(|host| {
                    origin == format!("http://{host}") || origin == format!("https://{host}")
                })
                .unwrap_or(false)
        })
        .unwrap_or(true);
    origin_ok && expected.as_deref().is_some_and(|v| v == submitted) && submitted.len() == 64
}
pub(super) fn set_session(cx: &Cx, token: &str) {
    safe_cookie_jar(cx).default_http_only(true).add(
        Cookie::build((SESSION, token.to_owned()))
            .max_age(topcoat::cookie::time::Duration::seconds(SESSION_AGE))
            .build(),
    );
}
pub(super) fn clear_session(cx: &Cx) {
    safe_cookie_jar(cx).remove(Cookie::build((SESSION, "")).http_only(true).build());
}
pub(super) fn auth_token(cx: &Cx) -> Option<String> {
    if let Some(value) = topcoat::router::request::headers(cx)
        .get("authorization")
        .and_then(|h| h.to_str().ok())
    {
        return value.strip_prefix("Bearer ").map(str::to_owned);
    }
    session_token(cx)
}
pub(super) async fn api_user(cx: &Cx) -> topcoat::Result<User> {
    let token = auth_token(cx).ok_or_else(unauthorized)?;
    authenticated_user(cx, &token)
        .await
        .clone()?
        .ok_or_else(|| unauthorized().into())
}

#[derive(Deserialize)]
pub(super) struct CsrfOnly {
    pub(super) csrf: String,
}

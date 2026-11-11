//! HTTP contracts that are easy to change accidentally during framework adoption.
use recipe_shelf::{core::App, web};
use topcoat::router::{Body, Router, request::Request, response::Response, to_bytes};

async fn send(
    router: &Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    fields: &[(&str, &str)],
) -> Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost");
    if let Some(token) = token {
        builder = builder.header(
            "cookie",
            format!("recipe_session={token}; recipe_csrf={}", "a".repeat(64)),
        );
    }
    let body = if fields.is_empty() {
        String::new()
    } else {
        builder = builder.header("content-type", "application/x-www-form-urlencoded");
        form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields.iter().copied())
            .finish()
    };
    router.handle(builder.body(Body::from(body)).unwrap()).await
}

async fn text(response: Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::test]
async fn query_contract_preserves_first_values_empty_strings_and_safe_return_urls() {
    let temporary = tempfile::tempdir().unwrap();
    let app = App::open(&temporary.path().join("test.sqlite")).unwrap();
    let admin = app.setup("admin", "test-password-123").unwrap();
    let folder = temporary.path().join("recipes");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(
        folder.join("recipe.toml"),
        "title = 'Café & Soup'\ndescription = 'A & B'\n",
    )
    .unwrap();
    app.add_folder(&admin, folder.to_str().unwrap()).unwrap();
    app.sync(&admin).unwrap();
    let id = app.recipes("", 1, 0).unwrap()[0].id;
    let token = app.login("admin", "test-password-123").unwrap().token;
    let router = web::router(app);
    for path in [
        "/?limit=",
        "/?offset=",
        "/?limit=-1",
        "/?offset=nope",
        "/?limit=&limit=2",
        "/settings/users?edit=18446744073709551615",
    ] {
        let response = send(&router, "GET", path, Some(&token), &[]).await;
        assert_eq!(response.status(), 400, "{path}");
    }
    for path in [
        "/?limit=2&limit=bad",
        "/?q=&q=ignored&sort=unknown",
        "/?limit=0&offset=0",
        "/settings?error=&saved=",
    ] {
        let response = send(&router, "GET", path, Some(&token), &[]).await;
        assert_eq!(response.status(), 200, "{path}");
        let html = text(response).await;
        if path.contains("q=") {
            assert!(html.contains("name=\"q\" value=\"\""));
            assert!(!html.contains("value=\"ignored\""));
        }
    }
    let response = send(
        &router,
        "GET",
        "/settings?error=folder&error=user",
        Some(&token),
        &[],
    )
    .await;
    assert!(text(response).await.contains("folder could not be changed"));
    for value in [
        "https://example.com/",
        "//example.com/",
        "/settings",
        "/%2fexample.com",
    ] {
        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair("from", value)
            .finish();
        let response = send(
            &router,
            "GET",
            &format!("/recipes/{id}?{query}"),
            Some(&token),
            &[],
        )
        .await;
        assert_eq!(response.status(), 200);
        assert!(text(response).await.contains("class=\"brand\" href=\"/\""));
    }
    let response = send(
        &router,
        "GET",
        "/?q=Caf%C3%A9+%26&limit=2",
        Some(&token),
        &[],
    )
    .await;
    let html = text(response).await;
    assert!(html.contains("Café &amp; Soup"));
    assert!(html.contains(&format!(
        "/recipes/{id}?from=%2F%3Fq%3DCaf%25C3%25A9%2B%2526"
    )));
}

#[tokio::test]
async fn infrastructure_failures_remain_server_errors_for_pages_api_and_forms() {
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("test.sqlite");
    let app = App::open(&database).unwrap();
    app.setup("admin", "test-password-123").unwrap();
    let token = app.login("admin", "test-password-123").unwrap().token;
    let connection = rusqlite::Connection::open(&database).unwrap();
    let router = web::router(app);
    connection.execute("DROP TABLE folders", []).unwrap();
    let csrf = "a".repeat(64);
    let response = send(
        &router,
        "POST",
        "/settings/folders",
        Some(&token),
        &[
            ("csrf", &csrf),
            ("path", temporary.path().to_str().unwrap()),
        ],
    )
    .await;
    assert_eq!(response.status(), 500);
    assert!(!text(response).await.contains("no such table"));
    connection.execute("DROP TABLE sessions", []).unwrap();
    for path in ["/", "/settings", "/api/recipes", "/recipes/1/export/text"] {
        let response = send(&router, "GET", path, Some(&token), &[]).await;
        assert_eq!(response.status(), 500, "{path}");
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = text(response).await;
        if path.starts_with("/api/") {
            assert_eq!(body, "{\"error\":\"request failed\"}");
        }
        assert!(!body.contains("no such table"));
    }
}

#[tokio::test]
async fn invalid_stored_passwords_do_not_consume_login_attempts() {
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("test.sqlite");
    let app = App::open(&database).unwrap();
    app.setup("admin", "test-password-123").unwrap();
    rusqlite::Connection::open(&database)
        .unwrap()
        .execute("UPDATE users SET password_hash='broken'", [])
        .unwrap();
    let router = web::router(app);
    for _ in 0..9 {
        let response = router
            .handle(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"username":"admin","password":"test-password-123"}"#,
                    ))
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), 500);
    }
}

#[tokio::test]
async fn login_attempt_policy_is_shared_by_web_and_api() {
    let temporary = tempfile::tempdir().unwrap();
    let app = App::open(&temporary.path().join("test.sqlite")).unwrap();
    app.setup("admin", "test-password-123").unwrap();
    let router = web::router(app);
    let csrf = "a".repeat(64);
    for _ in 0..4 {
        let web = send(
            &router,
            "POST",
            "/login",
            Some("unused"),
            &[
                ("csrf", &csrf),
                ("username", "admin"),
                ("password", "wrong-password"),
            ],
        )
        .await;
        assert_eq!(web.headers()["location"], "/login?error=credentials");
        let api = router
            .handle(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"username":"admin","password":"wrong-password"}"#,
                    ))
                    .unwrap(),
            )
            .await;
        assert_eq!(api.status(), 401);
    }
    let web = send(
        &router,
        "POST",
        "/login",
        Some("unused"),
        &[
            ("csrf", &csrf),
            ("username", "admin"),
            ("password", "test-password-123"),
        ],
    )
    .await;
    assert_eq!(web.headers()["location"], "/login?error=throttled");
    let api = router
        .handle(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"username":"admin","password":"test-password-123"}"#,
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(api.status(), 429);
    assert_eq!(api.headers()["retry-after"], "300");
}

#[tokio::test]
async fn typed_assets_are_hashed_immutable_and_embedded_with_legacy_aliases() {
    use sha2::{Digest, Sha256};
    let temporary = tempfile::tempdir().unwrap();
    let router = web::router(App::open(&temporary.path().join("test.sqlite")).unwrap());
    let html = text(send(&router, "GET", "/setup", None, &[]).await).await;
    let urls: Vec<_> = html
        .split('"')
        .filter(|part| part.starts_with("/_topcoat/assets/"))
        .collect();
    assert_eq!(urls.len(), 6);
    for url in urls {
        let response = send(&router, "GET", url, None, &[]).await;
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers()["cache-control"],
            "public, max-age=31536000, immutable"
        );
        let source = text(response).await;
        let digest = hex::encode(Sha256::digest(source.as_bytes()));
        assert!(url.contains(&digest[..16]));
    }
    for path in [
        "/assets/site.css",
        "/assets/theme.js",
        "/assets/app.js",
        "/assets/icon.svg",
        "/assets/manifest.webmanifest",
    ] {
        let response = send(&router, "GET", path, None, &[]).await;
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["cache-control"], "no-cache");
    }
    let response = send(&router, "GET", "/_topcoat/assets/missing.js", None, &[]).await;
    assert_eq!(response.status(), 404);
    assert_eq!(response.headers()["cache-control"], "no-store");
}

#[tokio::test]
async fn authentication_transport_precedence_and_csrf_origin_contract_are_preserved() {
    let temporary = tempfile::tempdir().unwrap();
    let app = App::open(&temporary.path().join("test.sqlite")).unwrap();
    app.setup("admin", "test-password-123").unwrap();
    let token = app.login("admin", "test-password-123").unwrap().token;
    let router = web::router(app);
    let cookie = format!("recipe_session={token}; recipe_csrf={}", "a".repeat(64));
    for authorization in ["Bearer invalid", "Basic ignored"] {
        let response = router
            .handle(
                Request::builder()
                    .uri("/api/recipes")
                    .header("cookie", &cookie)
                    .header("authorization", authorization)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), 401, "{authorization}");
    }
    let response = router
        .handle(
            Request::builder()
                .uri("/api/recipes")
                .header("cookie", "recipe_session=invalid")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), 200);
    let response = send(&router, "GET", "/api/recipes", Some(&token), &[]).await;
    assert_eq!(response.status(), 200);
    for origin in [
        None,
        Some("http://localhost"),
        Some("https://elsewhere.example"),
    ] {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/settings/sync")
            .header("host", "localhost")
            .header("cookie", &cookie)
            .header("content-type", "application/x-www-form-urlencoded");
        if let Some(origin) = origin {
            builder = builder.header("origin", origin);
        }
        let response = router
            .handle(
                builder
                    .body(Body::from(format!("csrf={}", "a".repeat(64))))
                    .unwrap(),
            )
            .await;
        assert_eq!(
            response.status().as_u16(),
            if origin == Some("https://elsewhere.example") {
                403
            } else {
                303
            }
        );
    }
    let response = send(
        &router,
        "POST",
        "/settings/sync",
        Some(&token),
        &[("csrf", "invalid")],
    )
    .await;
    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn app_favicon_and_manifest_share_the_svg_icon() {
    let temporary = tempfile::tempdir().unwrap();
    let router = web::router(App::open(&temporary.path().join("test.sqlite")).unwrap());
    let html = text(send(&router, "GET", "/setup", None, &[]).await).await;
    let favicon = html
        .split("<link ")
        .find(|tag| tag.contains("rel=\"icon\""))
        .unwrap();
    assert!(favicon.contains("image/svg+xml"));
    let icon_url = favicon
        .split("href=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let brand = html
        .split("<img ")
        .find(|tag| tag.contains("brand-mark"))
        .unwrap();
    assert!(brand.split('>').next().unwrap().contains(icon_url));
    let icon = send(&router, "GET", icon_url, None, &[]).await;
    assert_eq!(icon.status(), 200);
    assert_eq!(icon.headers()["content-type"], "image/svg+xml");
    let svg = text(icon).await;
    assert!(svg.contains("prefers-color-scheme: dark"));
    assert!(!svg.contains("<text"));
    let response = send(&router, "GET", "/assets/manifest.webmanifest", None, &[]).await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()["content-type"],
        "application/manifest+json"
    );
    let manifest: serde_json::Value = serde_json::from_str(&text(response).await).unwrap();
    assert_eq!(manifest["name"], "Recipe Shelf");
    assert_eq!(manifest["start_url"], "/");
    assert_eq!(manifest["scope"], "/");
    assert_eq!(manifest["display"], "standalone");
    let icons = manifest["icons"].as_array().unwrap();
    assert_eq!(icons.len(), 1);
    assert_eq!(icons[0]["sizes"], "any");
    assert_eq!(icons[0]["type"], "image/svg+xml");
    let response = send(&router, "GET", icons[0]["src"].as_str().unwrap(), None, &[]).await;
    assert_eq!(response.status(), 200);
    assert_eq!(text(response).await, svg);
}

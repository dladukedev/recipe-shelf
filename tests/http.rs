use recipe_shelf::{core::App, web};
use serde_json::{Value, json};
use topcoat::router::{Body, Router, request::Request, to_bytes};

async fn request(
    router: &Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (u16, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let response = router
        .handle(
            builder
                .body(Body::from(body.map(|v| v.to_string()).unwrap_or_default()))
                .unwrap(),
        )
        .await;
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"unexpected_body": String::from_utf8_lossy(&bytes)}));
    (status, value)
}

#[tokio::test]
async fn mobile_api_auth_search_pagination_and_revocation() {
    let temporary = tempfile::tempdir().unwrap();
    let recipes = temporary.path().join("recipes");
    std::fs::create_dir_all(recipes.join("nested")).unwrap();
    std::fs::write(
        recipes.join("bread.toml"),
        "title = 'Bread'\ningredients = 'flour, water'\ninstructions = ['Bake']\ntotal_time = 90\n# hidden searchable café 100%\n",
    )
    .unwrap();
    std::fs::write(
        recipes.join("nested/soup.toml"),
        "title = 'Soup'\ningredients = 'water'\n",
    )
    .unwrap();
    let app = App::open(&temporary.path().join("test.sqlite")).unwrap();
    let admin = app.setup("admin", "test-password-123").unwrap();
    app.create_user(&admin, "reader", "reader-password-123", false)
        .unwrap();
    app.add_folder(&admin, recipes.to_str().unwrap()).unwrap();
    assert_eq!(app.sync(&admin).unwrap().added, 2);
    let router = web::router(app);

    let (status, error) = request(&router, "GET", "/api/recipes", None, None).await;
    assert_eq!(status, 401, "{error}");
    assert!(error.get("error").is_some());
    let (status, error) = request(
        &router,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"username":"reader", "password":"incorrect-password"})),
    )
    .await;
    assert_eq!(status, 401, "{error}");
    let (status, login) = request(
        &router,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({"username":"reader", "password":"reader-password-123"})),
    )
    .await;
    assert_eq!(status, 200, "{login}");
    let token = login["token"].as_str().unwrap();
    assert_eq!(login["user"]["is_admin"], false);
    assert!(login["user"].get("password").is_none());

    for path in [
        "/api/recipes?limit=invalid",
        "/api/recipes?offset=-1",
        "/api/recipes/not-a-number",
    ] {
        let (status, error) = request(&router, "GET", path, Some(token), None).await;
        assert_eq!(status, 400, "{path}: {error}");
        assert!(error.get("error").is_some());
    }
    let (status, error) = request(&router, "GET", "/api/unknown", Some(token), None).await;
    assert_eq!(status, 404, "{error}");
    assert!(error.get("error").is_some());

    let (status, list) = request(&router, "GET", "/api/recipes", Some(token), None).await;
    assert_eq!(status, 200, "{list}");
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["data"]["title"], "Bread");
    assert_eq!(list[0]["data"]["total_time"], 90);
    assert!(list[0]["data"].get("prep_time").is_none());
    let id = list[0]["id"].as_i64().unwrap();
    let (status, single) = request(
        &router,
        "GET",
        &format!("/api/recipes/{id}"),
        Some(token),
        None,
    )
    .await;
    assert_eq!(status, 200, "{single}");
    assert_eq!(single, list[0]);

    for query in ["CAF%C3%89", "100%25", "hidden"] {
        let (status, found) = request(
            &router,
            "GET",
            &format!("/api/recipes?q={query}"),
            Some(token),
            None,
        )
        .await;
        assert_eq!(status, 200, "{found}");
        assert_eq!(found.as_array().unwrap().len(), 1, "{found}");
    }
    let (status, page) = request(
        &router,
        "GET",
        "/api/recipes?limit=1&offset=1",
        Some(token),
        None,
    )
    .await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page[0]["data"]["title"], "Soup");
    let (status, error) = request(&router, "GET", "/api/recipes/9999999", Some(token), None).await;
    assert_eq!(status, 404, "{error}");
    assert!(error.get("error").is_some());

    let (status, _) = request(
        &router,
        "POST",
        "/api/recipes",
        Some(token),
        Some(json!({"title":"Forbidden"})),
    )
    .await;
    assert_eq!(status, 405);
    let (status, logout) = request(&router, "POST", "/api/auth/logout", Some(token), None).await;
    assert!([200, 204].contains(&status), "{logout}");
    let (status, _) = request(&router, "GET", "/api/recipes", Some(token), None).await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn initial_setup_redirect_and_public_browsing_with_local_assets() {
    let temporary = tempfile::tempdir().unwrap();
    let app = App::open(&temporary.path().join("test.sqlite")).unwrap();
    let router = web::router(app.clone());
    let response = router
        .handle(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await;
    assert!(response.status().is_redirection());
    assert_eq!(response.headers()["location"], "/setup");
    app.setup("admin", "test-password-123").unwrap();
    let response = router
        .handle(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await;
    assert_eq!(response.status().as_u16(), 200);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains(">Login</a>"));
    assert!(!html.contains("Account options"));

    for path in ["/assets/site.css", "/assets/theme.js"] {
        let response = router
            .handle(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await;
        assert_eq!(response.status().as_u16(), 200);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert!(!bytes.is_empty());
    }
}

#[tokio::test]
async fn api_malformed_login_and_throttle_errors_are_json() {
    let temporary = tempfile::tempdir().unwrap();
    let app = App::open(&temporary.path().join("test.sqlite")).unwrap();
    app.setup("admin", "test-password-123").unwrap();
    let router = web::router(app);
    let response = router
        .handle(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from("{broken"))
                .unwrap(),
        )
        .await;
    assert_eq!(response.status().as_u16(), 400);
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(response.headers()["cache-control"], "no-store");
    for _ in 0..8 {
        let (status, error) = request(
            &router,
            "POST",
            "/api/auth/login",
            None,
            Some(json!({"username":"admin", "password":"invalid-password"})),
        )
        .await;
        assert_eq!(status, 401, "{error}");
    }
    let response = router
        .handle(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username":"admin", "password":"test-password-123"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(response.status().as_u16(), 429);
    assert_eq!(response.headers()["retry-after"], "300");
    assert_eq!(response.headers()["content-type"], "application/json");
}

#[tokio::test]
async fn health_is_public_before_and_after_setup() {
    let temporary = tempfile::tempdir().unwrap();
    let app = App::open(&temporary.path().join("test.sqlite")).unwrap();
    let router = web::router(app.clone());

    for configured in [false, true] {
        if configured {
            app.setup("admin", "test-password-123").unwrap();
        }
        let response = router
            .handle(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(response.headers()["content-type"], "application/json");
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert!(!response.headers().contains_key("set-cookie"));
        let bytes = to_bytes(response.into_body(), 1024).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            json!({"status": "ok"})
        );
    }
}

use std::collections::BTreeMap;

use recipe_shelf::{core::App, web};
use topcoat::router::{Body, Router, request::Request, to_bytes};

#[derive(Default)]
struct Browser {
    cookies: BTreeMap<String, String>,
}

impl Browser {
    async fn send(
        &mut self,
        router: &Router,
        method: &str,
        path: &str,
        fields: &[(&str, &str)],
    ) -> (u16, String, Option<String>) {
        let body = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields.iter().copied())
            .finish();
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("host", "localhost")
            .header("origin", "http://localhost")
            .header(
                "cookie",
                self.cookies
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
        if method == "POST" {
            request = request.header("content-type", "application/x-www-form-urlencoded");
        }
        let response = router.handle(request.body(Body::from(body)).unwrap()).await;
        for cookie in response.headers().get_all("set-cookie") {
            let cookie = cookie.to_str().unwrap();
            let (name, value) = cookie.split(';').next().unwrap().split_once('=').unwrap();
            if name == "recipe_session" && !value.is_empty() {
                assert!(cookie.contains("HttpOnly"), "{cookie}");
                assert!(cookie.contains("SameSite=Strict"), "{cookie}");
            }
            self.cookies.insert(name.into(), value.into());
        }
        let status = response.status().as_u16();
        let location = response
            .headers()
            .get("location")
            .map(|s| s.to_str().unwrap().into());
        let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap();
        (status, String::from_utf8(bytes.into()).unwrap(), location)
    }
}

#[tokio::test]
async fn browser_setup_admin_sync_reader_permissions_and_csrf() {
    let temporary = tempfile::tempdir().unwrap();
    let app = App::open(&temporary.path().join("test.sqlite")).unwrap();
    let router = web::router(app.clone());
    let mut browser = Browser::default();
    let (status, html, _) = browser.send(&router, "GET", "/setup", &[]).await;
    assert_eq!(status, 200, "{html}");
    assert!(html.contains("name=\"csrf\""));
    let csrf = browser.cookies["recipe_csrf"].clone();

    let (status, _, _) = browser
        .send(
            &router,
            "POST",
            "/setup",
            &[
                ("username", "admin"),
                ("password", "test-password-123"),
                ("csrf", "invalid"),
            ],
        )
        .await;
    assert_eq!(status, 403);
    assert!(app.needs_setup().unwrap());
    let (status, html, _) = browser
        .send(
            &router,
            "POST",
            "/setup",
            &[
                ("username", "admin"),
                ("password", "test-password-123"),
                ("csrf", &csrf),
            ],
        )
        .await;
    assert!(status == 200 || (300..400).contains(&status), "{html}");
    assert!(!app.needs_setup().unwrap());
    let (status, _, location) = browser
        .send(
            &router,
            "POST",
            "/login",
            &[
                ("username", "admin"),
                ("password", "incorrect-password"),
                ("csrf", &csrf),
            ],
        )
        .await;
    assert_eq!(
        status, 303,
        "failed login must redirect to a GET, avoiding repeated POSTs"
    );
    assert_eq!(location.as_deref(), Some("/login?error=credentials"));
    let (_, html, _) = browser
        .send(&router, "GET", "/login?error=credentials", &[])
        .await;
    assert!(html.contains("The username or password is incorrect"));
    let (status, html, location) = browser
        .send(
            &router,
            "POST",
            "/login",
            &[
                ("username", "admin"),
                ("password", "test-password-123"),
                ("csrf", &csrf),
            ],
        )
        .await;
    assert!((300..400).contains(&status), "{status} {html}");
    assert_eq!(location.as_deref(), Some("/"));
    assert!(!browser.cookies["recipe_session"].is_empty());
    let (status, settings, _) = browser.send(&router, "GET", "/settings", &[]).await;
    assert_eq!(status, 200, "{settings}");
    assert!(settings.contains("/settings/folders"));
    assert!(settings.contains("/settings/users"));
    assert!(settings.contains("Custom Theme"));
    assert!(settings.contains("data-custom-theme-toggle"));
    assert!(settings.contains("Light theme"));
    assert!(settings.contains("Dark theme"));
    assert_eq!(
        settings.matches("data-custom-theme-mode=\"light\"").count(),
        8
    );
    assert_eq!(
        settings.matches("data-custom-theme-mode=\"dark\"").count(),
        8
    );

    let csrf = browser.cookies["recipe_csrf"].clone();

    let recipes = temporary.path().join("recipes");
    std::fs::create_dir(&recipes).unwrap();
    std::fs::write(
        recipes.join("example.toml"),
        "title = '<script>alert(1)</script>'\ningredients = 'plain text'\n",
    )
    .unwrap();
    let (status, html, _) = browser
        .send(
            &router,
            "POST",
            "/settings/folders",
            &[("path", recipes.to_str().unwrap()), ("csrf", &csrf)],
        )
        .await;
    assert!((300..400).contains(&status), "{status} {html}");
    let (status, html, _) = browser
        .send(&router, "POST", "/settings/sync", &[("csrf", &csrf)])
        .await;
    assert_eq!(status, 303, "{html}");
    assert_eq!(app.recipes("", 50, 0).unwrap().len(), 1);
    let (status, html, _) = browser.send(&router, "GET", "/", &[]).await;
    assert_eq!(status, 200, "{html}");
    assert!(!html.contains("<script>alert(1)</script>"));
    assert!(
        html.contains("&lt;script&gt;"),
        "recipe title must be escaped"
    );

    let (status, html, _) = browser
        .send(
            &router,
            "POST",
            "/settings/users",
            &[
                ("username", "reader"),
                ("password", "reader-password-123"),
                ("csrf", &csrf),
            ],
        )
        .await;
    assert!((300..400).contains(&status), "{status} {html}");
    let (status, _, _) = browser
        .send(&router, "POST", "/logout", &[("csrf", &csrf)])
        .await;
    assert!((300..400).contains(&status));

    let mut reader = Browser::default();
    reader.send(&router, "GET", "/login", &[]).await;
    let csrf = reader.cookies["recipe_csrf"].clone();
    let (status, html, _) = reader
        .send(
            &router,
            "POST",
            "/login",
            &[
                ("username", "reader"),
                ("password", "reader-password-123"),
                ("csrf", &csrf),
            ],
        )
        .await;
    assert!((300..400).contains(&status), "{status} {html}");
    let (status, html, _) = reader.send(&router, "GET", "/settings", &[]).await;
    assert_eq!(status, 200, "{html}");
    assert!(!html.contains("/settings/folders"));
    assert!(!html.contains("/settings/users"));
    let csrf = reader.cookies["recipe_csrf"].clone();
    let (status, _, _) = reader
        .send(&router, "POST", "/settings/sync", &[("csrf", &csrf)])
        .await;
    assert_eq!(status, 403);
    let (status, _, _) = reader
        .send(
            &router,
            "POST",
            "/settings/users",
            &[
                ("username", "intruder"),
                ("password", "reader-password-123"),
                ("is_admin", "true"),
                ("csrf", &csrf),
            ],
        )
        .await;
    assert_eq!(status, 403);
}

#[test]
fn large_catalog_indexes_without_external_recipe_files() {
    let temporary = tempfile::tempdir().unwrap();
    let app = App::open(&temporary.path().join("test.sqlite")).unwrap();
    let admin = app.setup("admin", "test-password-123").unwrap();
    let recipes = temporary.path().join("recipes");
    std::fs::create_dir_all(recipes.join("nested")).unwrap();
    for index in 0..160 {
        std::fs::write(
            recipes.join(format!("recipe-{index}.toml")),
            format!("title = 'Recipe {index:03}'\ndescription = 'Catalog example'\n"),
        )
        .unwrap();
    }
    std::fs::write(
        recipes.join("nested/risotto.toml"),
        "title = 'Mushroom risotto'\nauthor = 'Sample cook'\nprep_time = 25\ncook_time = 23\ntotal_time = 48\ningredients = '1 cup rice and mushrooms'\n",
    ).unwrap();
    app.add_folder(&admin, recipes.to_str().unwrap()).unwrap();
    let report = app.sync(&admin).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.added, 161);
    assert_eq!(app.recipes("", 500, 0).unwrap().len(), 161);
    let risotto = app.recipes("Mushroom risotto", 10, 0).unwrap();
    assert_eq!(risotto.len(), 1);
    assert_eq!(risotto[0].data.author, "Sample cook");
    assert_eq!(risotto[0].data.prep_time, Some(25));
    assert_eq!(risotto[0].data.cook_time, Some(23));
    assert_eq!(risotto[0].data.total_time, Some(48));
    assert_eq!(risotto[0].data.ingredients, "1 cup rice and mushrooms");
    let unchanged = app.sync(&admin).unwrap();
    assert_eq!(
        (unchanged.added, unchanged.updated, unchanged.removed),
        (0, 0, 0)
    );
}

#[tokio::test]
async fn recipe_times_render_as_hours_and_minutes() {
    let temporary = tempfile::tempdir().unwrap();
    let app = App::open(&temporary.path().join("test.sqlite")).unwrap();
    let admin = app.setup("admin", "test-password-123").unwrap();
    let recipes = temporary.path().join("recipes");
    std::fs::create_dir(&recipes).unwrap();
    std::fs::write(
        recipes.join("timed.toml"),
        "title = 'Timed recipe'\nprep_time = 25\ncook_time = 120\ntotal_time = 145\n",
    )
    .unwrap();
    std::fs::write(recipes.join("unknown.toml"), "title = 'Unknown time'\n").unwrap();
    app.add_folder(&admin, recipes.to_str().unwrap()).unwrap();
    let report = app.sync(&admin).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let id = app.recipes("Timed recipe", 1, 0).unwrap()[0].id;
    let unknown = app.recipes("Unknown time", 1, 0).unwrap()[0].id;
    let login = app.login("admin", "test-password-123").unwrap();
    let router = web::router(app);
    let mut browser = Browser::default();
    browser.cookies.insert("recipe_session".into(), login.token);
    let (status, html, _) = browser.send(&router, "GET", "/", &[]).await;
    assert_eq!(status, 200);
    assert!(!html.contains("2 hrs 25 min"), "list rows omit timing");
    let (status, html, _) = browser
        .send(&router, "GET", &format!("/recipes/{id}"), &[])
        .await;
    assert_eq!(status, 200);
    for expected in ["Prep 25 min", "Cook 2 hrs", "Total 2 hrs 25 min"] {
        assert!(html.contains(expected), "{html}");
    }
    let (_, html, _) = browser
        .send(&router, "GET", &format!("/recipes/{unknown}"), &[])
        .await;
    assert!(!html.contains("Total 0 min"));
    assert!(
        html.contains("class=\"detail-rule\""),
        "recipes without metadata still have a divider"
    );
}

fn signed_in(app: &App, username: &str, password: &str) -> Browser {
    let mut browser = Browser::default();
    browser.cookies.insert(
        "recipe_session".into(),
        app.login(username, password).unwrap().token,
    );
    browser.cookies.insert("recipe_csrf".into(), "a".repeat(64));
    browser
}

#[tokio::test]
async fn graphite_browsing_filters_sorts_and_preserves_return_urls() {
    let temp = tempfile::tempdir().unwrap();
    let app = App::open(&temp.path().join("test.sqlite")).unwrap();
    let admin = app.setup("admin", "test-password-123").unwrap();
    let root = temp.path().join("recipes");
    std::fs::create_dir(&root).unwrap();
    for (name, text) in [
        (
            "a",
            "title='Apple soup'\ndescription='Bright and warming'\ningredients='secret ingredient'\ncategories=['Lunch']\ntotal_time=90",
        ),
        (
            "b",
            "title='Bean soup'\ndescription='A quick lunch'\ncategories=['Lunch']\ntotal_time=10",
        ),
        ("c", "title='Cake'\ncategories=['Baking']"),
    ] {
        std::fs::write(root.join(format!("{name}.toml")), text).unwrap();
    }
    app.add_folder(&admin, root.to_str().unwrap()).unwrap();
    app.sync(&admin).unwrap();
    let mut browser = signed_in(&app, "admin", "test-password-123");
    let router = web::router(app.clone());
    let (status, html, _) = browser
        .send(&router, "GET", "/?sort=time&limit=1", &[])
        .await;
    assert_eq!(status, 200);
    assert!(html.contains("Bean soup"));
    assert!(!html.contains("Apple soup"));
    assert!(html.contains("3 recipes"));
    assert!(html.contains("category=&amp;sort=time&amp;limit=1&amp;offset=1"));
    let (_, html, _) = browser.send(&router, "GET", "/?q=secret", &[]).await;
    assert!(html.contains("No recipes found"));
    let (_, html, _) = browser.send(&router, "GET", "/?q=warming", &[]).await;
    assert!(html.contains("Apple soup"));
    assert!(!html.contains("Bean soup"));
    let (_, html, _) = browser.send(&router, "GET", "/?category=Baking", &[]).await;
    assert!(html.contains("Cake"));
    assert!(!html.contains("Apple soup"));
    assert!(html.contains("Export all"));
    assert!(html.contains("id=\"recipe-sort\""));
    let id = app.recipes("Apple soup", 1, 0).unwrap()[0].id;
    let (_, html, _) = browser
        .send(
            &router,
            "GET",
            &format!("/recipes/{id}?from=%2F%3Fq%3Dsoup%26sort%3Dtime"),
            &[],
        )
        .await;
    assert!(html.contains("href=\"/?q=soup&amp;sort=time\""));
    assert!(html.contains(&format!("/recipes/{id}/export/pdf")));
    assert!(!html.contains("Print PDF"));
    assert!(!html.contains("type=\"checkbox\""));
    let (_, html, _) = browser
        .send(
            &router,
            "GET",
            &format!("/recipes/{id}?from=%2F%2Fevil.example"),
            &[],
        )
        .await;
    assert!(!html.contains("evil.example"));
    let (_, html, _) = browser.send(&router, "GET", "/exports", &[]).await;
    assert!(!html.contains("One ZIP archive"));
    assert!(html.contains("/settings/templates"));
    assert_eq!(
        browser
            .send(&router, "GET", "/exports/download?format=pdf", &[])
            .await
            .2
            .as_deref(),
        Some("/settings/export/pdf")
    );
    assert_eq!(
        browser
            .send(&router, "GET", "/exports/download?format=invalid", &[])
            .await
            .0,
        400
    );
}

#[tokio::test]
async fn graphite_user_management_and_password_changes_enforce_permissions() {
    let temp = tempfile::tempdir().unwrap();
    let app = App::open(&temp.path().join("test.sqlite")).unwrap();
    let admin = app.setup("admin", "test-password-123").unwrap();
    let reader = app
        .create_user(&admin, "reader", "reader-password-123", false)
        .unwrap();
    let old_session = app.login("reader", "reader-password-123").unwrap().token;
    let mut browser = signed_in(&app, "admin", "test-password-123");
    let mut member = signed_in(&app, "reader", "reader-password-123");
    let router = web::router(app.clone());
    let csrf = "a".repeat(64);
    for path in [
        "/settings/folders",
        "/settings/users",
        "/settings/templates",
    ] {
        assert_eq!(
            member.send(&router, "GET", path, &[]).await.0,
            403,
            "{path}"
        );
    }
    for path in ["/settings/users/delete", "/settings/folders/remove"] {
        assert_eq!(
            member
                .send(&router, "POST", path, &[("csrf", &csrf), ("id", "1")])
                .await
                .0,
            403,
            "{path}"
        );
        assert_eq!(
            browser
                .send(&router, "POST", path, &[("csrf", "wrong"), ("id", "1")])
                .await
                .0,
            403,
            "{path}"
        );
    }
    let id = reader.id.to_string();
    let fields = [
        ("csrf", csrf.as_str()),
        ("id", id.as_str()),
        ("username", "renamed"),
        ("password", "reset-password-123"),
    ];
    assert_eq!(
        member
            .send(&router, "POST", "/settings/users/update", &fields)
            .await
            .0,
        403
    );
    assert_eq!(
        browser
            .send(&router, "POST", "/settings/users/update", &fields)
            .await
            .0,
        303
    );
    assert!(app.login("reader", "reader-password-123").is_err());
    assert!(app.login("renamed", "reset-password-123").is_ok());
    assert!(app.authenticate(&old_session).unwrap().is_none());
    let fields = [("csrf", csrf.as_str()), ("id", "1")];
    assert!(
        browser
            .send(&router, "POST", "/settings/users/delete", &fields)
            .await
            .2
            .unwrap()
            .contains("error=user")
    );
    assert_eq!(app.users(&admin).unwrap().len(), 2);
    let fields = [("csrf", csrf.as_str()), ("id", id.as_str())];
    assert_eq!(
        browser
            .send(&router, "POST", "/settings/users/delete", &fields)
            .await
            .0,
        303
    );
    assert!(app.login("renamed", "reset-password-123").is_err());

    let member_user = app
        .create_user(&admin, "member2", "member-password-123", false)
        .unwrap();
    let mut member = signed_in(&app, "member2", "member-password-123");
    let session = member.cookies["recipe_session"].clone();
    let (_, html, _) = member.send(&router, "GET", "/settings", &[]).await;
    assert!(html.contains("/settings/password"));
    assert!(!html.contains("Settings sections"));
    for (current, confirmation, error) in [
        ("member-password-123", "mismatch", "password-match"),
        ("incorrect", "new-password-123", "password"),
    ] {
        let (_, html, location) = member
            .send(
                &router,
                "POST",
                "/settings/password",
                &[
                    ("csrf", &csrf),
                    ("current", current),
                    ("password", "new-password-123"),
                    ("confirm", confirmation),
                ],
            )
            .await;
        assert!(location.unwrap().ends_with(error));
        assert!(!html.contains("new-password-123"));
        assert!(app.authenticate(&session).unwrap().is_some());
    }
    assert_eq!(
        member
            .send(
                &router,
                "POST",
                "/settings/password",
                &[
                    ("csrf", "bad"),
                    ("current", "member-password-123"),
                    ("password", "new-password-123"),
                    ("confirm", "new-password-123")
                ]
            )
            .await
            .0,
        403
    );
    let (_, _, location) = member
        .send(
            &router,
            "POST",
            "/settings/password",
            &[
                ("csrf", &csrf),
                ("current", "member-password-123"),
                ("password", "new-password-123"),
                ("confirm", "new-password-123"),
            ],
        )
        .await;
    assert_eq!(location.as_deref(), Some("/login?password=changed"));
    assert!(app.authenticate(&session).unwrap().is_none());
    assert!(
        app.login(&member_user.username, "member-password-123")
            .is_err()
    );
    assert!(app.login(&member_user.username, "new-password-123").is_ok());
}

#[tokio::test]
async fn graphite_folder_reports_persist_and_removal_keeps_source_files() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("test.sqlite");
    let app = App::open(&db_path).unwrap();
    let admin = app.setup("admin", "test-password-123").unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    let source = "title='Soup'\ningredients='water'";
    std::fs::write(first.join("soup.toml"), source).unwrap();
    std::fs::write(second.join("cake.toml"), "title='Cake'").unwrap();
    let folder = app.add_folder(&admin, first.to_str().unwrap()).unwrap();
    app.add_folder(&admin, second.to_str().unwrap()).unwrap();
    app.sync(&admin).unwrap();
    let reports = app.folders(&admin).unwrap();
    assert!(
        reports
            .iter()
            .all(|f| f.recipe_count == 1 && f.last_sync.as_ref().unwrap().added == 1)
    );
    std::fs::rename(&second, temp.path().join("unmounted")).unwrap();
    assert_eq!(app.sync(&admin).unwrap().errors.len(), 1);
    let reopened = App::open(&db_path).unwrap();
    assert_eq!(
        reopened
            .folders(&admin)
            .unwrap()
            .iter()
            .filter(|f| !f.last_sync.as_ref().unwrap().errors.is_empty())
            .count(),
        1
    );
    assert_eq!(
        app.recipes("", 50, 0).unwrap().len(),
        2,
        "failed folder retains cached recipes"
    );
    let mut browser = signed_in(&app, "admin", "test-password-123");
    let router = web::router(app.clone());
    let (_, html, _) = browser.send(&router, "GET", "/settings/folders", &[]).await;
    assert!(html.contains("folder unavailable"));
    assert!(html.contains("1 recipes"));
    let csrf = "a".repeat(64);
    let id = folder.id.to_string();
    assert_eq!(
        browser
            .send(
                &router,
                "POST",
                "/settings/folders/remove",
                &[("csrf", &csrf), ("id", &id)]
            )
            .await
            .0,
        303
    );
    assert_eq!(
        std::fs::read_to_string(first.join("soup.toml")).unwrap(),
        source
    );
    assert_eq!(app.recipes("", 50, 0).unwrap().len(), 1);
    assert_eq!(app.folders(&admin).unwrap().len(), 1);
}

#[tokio::test]
async fn graphite_template_preview_uses_real_renderer_without_saving() {
    let temp = tempfile::tempdir().unwrap();
    let app = App::open(&temp.path().join("test.sqlite")).unwrap();
    let admin = app.setup("admin", "test-password-123").unwrap();
    app.create_user(&admin, "reader", "reader-password-123", false)
        .unwrap();
    let root = temp.path().join("recipes");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("soup.toml"),
        "title='<Soup>'\ninstructions=['Boil']",
    )
    .unwrap();
    app.add_folder(&admin, root.to_str().unwrap()).unwrap();
    app.sync(&admin).unwrap();
    let mut browser = signed_in(&app, "admin", "test-password-123");
    let mut member = signed_in(&app, "reader", "reader-password-123");
    let saved = app.export_templates().unwrap().text;
    let router = web::router(app.clone());
    let csrf = "a".repeat(64);
    let fields = [
        ("csrf", csrf.as_str()),
        ("format", "text"),
        (
            "source",
            "{{title}}:{{#instructions}}{{.}}{{/instructions}}",
        ),
    ];
    assert_eq!(
        member
            .send(&router, "POST", "/settings/templates/preview", &fields)
            .await
            .0,
        403
    );
    let (status, body, _) = browser
        .send(&router, "POST", "/settings/templates/preview", &fields)
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap()["preview"],
        "&lt;Soup&gt;:Boil"
    );
    assert_eq!(app.export_templates().unwrap().text, saved);
    let (_, html, _) = browser
        .send(&router, "GET", "/settings/templates?format=text", &[])
        .await;
    assert!(html.contains("data-template-form"));
    assert!(html.contains("template-preview"));
    let (_, body, _) = browser
        .send(
            &router,
            "POST",
            "/settings/templates/preview",
            &[
                ("csrf", &csrf),
                ("format", "text"),
                ("source", "{{#unclosed}}"),
            ],
        )
        .await;
    assert!(serde_json::from_str::<serde_json::Value>(&body).unwrap()["error"].is_string());
}

#[test]
fn user_edits_are_atomic_and_role_changes_revoke_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let app = App::open(&temp.path().join("test.sqlite")).unwrap();
    let admin = app.setup("admin", "test-password-123").unwrap();
    let second = app
        .create_user(&admin, "second", "second-password-123", true)
        .unwrap();
    let member = app
        .create_user(&admin, "member", "member-password-123", false)
        .unwrap();
    let login = app.login("second", "second-password-123").unwrap();
    assert!(
        app.update_user(&member, second.id, "second", None, false)
            .is_err()
    );
    assert!(
        app.update_user(
            &admin,
            second.id,
            "ADMIN",
            Some("reset-password-123"),
            false
        )
        .is_err()
    );
    assert!(
        app.authenticate(&login.token).unwrap().is_some(),
        "failed rename must not revoke sessions or change password/role"
    );
    assert!(
        app.login("second", "second-password-123")
            .unwrap()
            .user
            .is_admin
    );
    app.update_user(&admin, second.id, "second", None, false)
        .unwrap();
    assert!(app.authenticate(&login.token).unwrap().is_none());
    assert!(
        !app.login("second", "second-password-123")
            .unwrap()
            .user
            .is_admin
    );
    assert!(
        app.delete_user(&second, member.id).is_err(),
        "stale admin data must not grant permissions"
    );
    assert!(
        app.update_user(&admin, admin.id, "admin", None, false)
            .is_err()
    );
    assert!(app.delete_user(&admin, admin.id).is_err());
}

fn dialog_open(html: &str, id: &str) -> bool {
    html.split(&format!("<dialog id=\"{id}\""))
        .nth(1)
        .unwrap()
        .split('>')
        .next()
        .unwrap()
        .contains(" open")
}

#[tokio::test]
async fn compact_pages_keep_actions_in_header_modals_and_snackbars() {
    let temporary = tempfile::tempdir().unwrap();
    let app = App::open(&temporary.path().join("test.sqlite")).unwrap();
    let admin = app.setup("admin", "test-password-123").unwrap();
    let member = app
        .create_user(&admin, "reader", "reader-password-123", false)
        .unwrap();
    let root = temporary.path().join("recipes");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("soup.toml"), "title='Soup'\ntotal_time=90\n").unwrap();
    app.add_folder(&admin, root.to_str().unwrap()).unwrap();
    app.sync(&admin).unwrap();
    let id = app.recipes("", 1, 0).unwrap()[0].id;
    let mut browser = signed_in(&app, "admin", "test-password-123");
    let router = web::router(app);
    let (_, html, _) = browser.send(&router, "GET", "/", &[]).await;
    let header = html.split("</header>").next().unwrap();
    assert!(!header.contains("page-context"));
    assert!(header.contains("/exports"));
    assert!(!html.contains("page-intro"));
    assert!(!html.contains("1 hr 30 min"));
    assert!(!html.contains("card-meta"));
    let (_, html, _) = browser
        .send(&router, "GET", &format!("/recipes/{id}"), &[])
        .await;
    assert!(!html.contains("Back to Recipes"));
    assert!(html.contains("detail-title-row"));
    assert!(
        !html
            .split("</header>")
            .next()
            .unwrap()
            .contains("page-context")
    );
    assert!(html.contains("class=\"detail-rule\""));
    assert!(
        html.contains("1 hr 30 min"),
        "detail timing remains available"
    );
    let (_, html, _) = browser.send(&router, "GET", "/settings?saved=1", &[]).await;
    assert!(html.contains(" - Settings"));
    assert!(!html.contains("page-intro"));
    assert!(!html.contains("Back to Recipes"));
    assert!(html.contains("data-snackbar"));
    assert!(!dialog_open(&html, "password-dialog"));
    let (_, html, _) = browser
        .send(&router, "GET", "/settings?modal=password", &[])
        .await;
    assert!(dialog_open(&html, "password-dialog"));
    let (_, html, _) = browser
        .send(&router, "GET", "/settings?error=password-match", &[])
        .await;
    assert!(dialog_open(&html, "password-dialog"));
    assert!(html.contains("new passwords do not match"));
    let (_, html, _) = browser.send(&router, "GET", "/settings/users", &[]).await;
    assert!(!dialog_open(&html, "user-dialog"));
    let (_, html, _) = browser
        .send(&router, "GET", "/settings/users?add=1", &[])
        .await;
    assert!(dialog_open(&html, "user-dialog"));
    let (_, html, _) = browser
        .send(
            &router,
            "GET",
            &format!("/settings/users?edit={}", member.id),
            &[],
        )
        .await;
    assert!(dialog_open(&html, "user-dialog"));
    assert!(html.contains("action=\"/settings/users/update\""));
    assert!(html.contains("value=\"reader\""));
}

#[tokio::test]
async fn saved_and_draft_previews_select_recipes_without_granting_template_access() {
    let temporary = tempfile::tempdir().unwrap();
    let app = App::open(&temporary.path().join("test.sqlite")).unwrap();
    let admin = app.setup("admin", "test-password-123").unwrap();
    app.create_user(&admin, "reader", "reader-password-123", false)
        .unwrap();
    let root = temporary.path().join("recipes");
    std::fs::create_dir(&root).unwrap();
    for (file, title) in [("a", "Alpha"), ("b", "Beta")] {
        std::fs::write(
            root.join(format!("{file}.toml")),
            format!("title='{title}'\n"),
        )
        .unwrap();
    }
    app.add_folder(&admin, root.to_str().unwrap()).unwrap();
    app.sync(&admin).unwrap();
    let id = app.recipes("Beta", 1, 0).unwrap()[0].id;
    app.save_export_template(&admin, "text", "SAVED TEXT {{title}}")
        .unwrap();
    app.save_export_template(&admin, "pdf", "<h1>PDF {{title}}</h1>")
        .unwrap();
    let mut browser = signed_in(&app, "admin", "test-password-123");
    let mut member = signed_in(&app, "reader", "reader-password-123");
    let mut guest = Browser::default();
    let router = web::router(app.clone());
    let (status, html, _) = browser
        .send(
            &router,
            "GET",
            &format!("/exports?format=text&recipe={id}"),
            &[],
        )
        .await;
    assert_eq!(status, 200);
    assert!(html.contains("SAVED TEXT Beta"));
    assert!(html.contains("/settings/templates?format=text"));
    assert!(!html.contains("Export your collection"));
    assert!(!html.contains("Back to Recipes"));
    assert!(!html.contains("Sample from your collection"));
    assert!(!html.contains("Uses your saved export templates."));
    assert!(!html.contains("One ZIP archive"));
    let card = html
        .split("<aside class=\"panel export-preview\"")
        .nth(1)
        .unwrap();
    assert!(card.contains("id=\"export-preview-recipe\""));
    assert!(card.contains("form=\"collection-export\""));

    let (_, html, _) = browser
        .send(
            &router,
            "GET",
            &format!("/exports?format=pdf&recipe={id}"),
            &[],
        )
        .await;
    assert!(html.contains("&lt;h1&gt;PDF Beta&lt;/h1&gt;"));
    assert!(html.contains("/settings/templates?format=pdf"));
    let (_, body, _) = member
        .send(
            &router,
            "GET",
            &format!("/exports/preview?format=text&recipe={id}&source=UNTRUSTED"),
            &[],
        )
        .await;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap()["preview"],
        "SAVED TEXT Beta"
    );
    assert_eq!(
        guest.send(&router, "GET", "/exports/preview", &[]).await.0,
        401
    );
    for path in [
        "/exports/preview?format=bad",
        "/exports/preview?recipe=bad",
        "/exports/preview?recipe=-1",
    ] {
        assert_eq!(member.send(&router, "GET", path, &[]).await.0, 400);
    }
    assert_eq!(
        member
            .send(&router, "GET", "/exports/preview?recipe=99999", &[])
            .await
            .0,
        404
    );
    let (_, html, _) = browser
        .send(
            &router,
            "GET",
            &format!("/settings/templates?format=text&recipe={id}"),
            &[],
        )
        .await;
    assert!(html.contains("template-preview-recipe"));
    assert!(html.contains("SAVED TEXT Beta"));
    assert!(!html.contains(">Administrator</span>"));
    assert!(
        html.contains(&format!("format=pdf&amp;recipe={id}")),
        "template tabs retain the selected recipe"
    );

    let csrf = "a".repeat(64);
    let id = id.to_string();
    let fields = [
        ("csrf", csrf.as_str()),
        ("format", "text"),
        ("source", "DRAFT {{title}}"),
        ("recipe", id.as_str()),
    ];
    let (status, body, _) = browser
        .send(&router, "POST", "/settings/templates/preview", &fields)
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap()["preview"],
        "DRAFT Beta"
    );
    assert_eq!(app.export_templates().unwrap().text, "SAVED TEXT {{title}}");
    let save_fields = [
        ("csrf", csrf.as_str()),
        ("format", "text"),
        ("source", "SAVED TEXT {{title}}"),
        ("recipe", id.as_str()),
    ];
    let (status, _, location) = browser
        .send(&router, "POST", "/settings/templates", &save_fields)
        .await;
    assert_eq!(status, 303);
    assert!(location.unwrap().contains(&format!("recipe={id}")));

    assert_eq!(
        member
            .send(&router, "POST", "/settings/templates/preview", &fields)
            .await
            .0,
        403
    );
}

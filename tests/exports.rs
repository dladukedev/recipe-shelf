use std::process::Command;

use recipe_shelf::{
    core::{App, Recipe},
    exports, mustache, web,
};
use serde_json::Value;
use topcoat::router::{Body, HeaderMap, Router, request::Request, to_bytes};

fn fixture() -> (tempfile::TempDir, App, String, String, Vec<Recipe>) {
    let temporary = tempfile::tempdir().unwrap();
    let app = App::open(&temporary.path().join("db.sqlite")).unwrap();
    let admin = app.setup("admin", "test-password-123").unwrap();
    app.create_user(&admin, "reader", "reader-password-123", false)
        .unwrap();
    let recipes = temporary.path().join("recipes");
    std::fs::create_dir_all(recipes.join("nested")).unwrap();
    for relative in ["one.toml", "nested/two.toml"] {
        std::fs::write(recipes.join(relative), "title = '<Soup> & crème'\ningredients = \"½ cup rice\\n2 cups water\"\ninstructions = ['Mix <gently>', 'Cook thoroughly']\nnotes = ['A & B']\nprep_time = 0\ntotal_time = 90\n").unwrap();
    }
    app.add_folder(&admin, recipes.to_str().unwrap()).unwrap();
    assert_eq!(app.sync(&admin).unwrap().added, 2);
    let catalog = app.export_recipes().unwrap();
    let admin_token = app.login("admin", "test-password-123").unwrap().token;
    let reader_token = app.login("reader", "reader-password-123").unwrap().token;
    (temporary, app, admin_token, reader_token, catalog)
}

async fn send(
    router: &Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    fields: &[(&str, &str)],
) -> (u16, HeaderMap, Vec<u8>) {
    let form = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields.iter().copied())
        .finish();
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost")
        .header("origin", "http://localhost");
    if let Some(token) = token {
        builder = builder.header(
            "cookie",
            format!("recipe_session={token}; recipe_csrf={}", "a".repeat(64)),
        );
    }
    if method == "POST" {
        builder = builder.header("content-type", "application/x-www-form-urlencoded");
    }
    let response = router.handle(builder.body(Body::from(form)).unwrap()).await;
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    (status, headers, bytes)
}

fn zip_contents(bytes: &[u8]) -> Vec<Value> {
    let temporary = tempfile::tempdir().unwrap();
    let archive = temporary.path().join("recipes.zip");
    std::fs::write(&archive, bytes).unwrap();
    let output = Command::new("python3").args(["-c", "import json,sys,zipfile\nwith zipfile.ZipFile(sys.argv[1]) as z:\n assert z.testzip() is None\n print(json.dumps([{'name':n,'text':z.read(n).decode('utf-8')} for n in z.namelist()]))"]).arg(archive).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[tokio::test]
async fn authenticated_members_download_single_files_and_complete_zip() {
    let (_temporary, app, _admin, reader, catalog) = fixture();
    let router = web::router(app);
    for format in ["text", "markdown"] {
        let path = format!("/recipes/{}/export/{format}", catalog[0].id);
        assert_eq!(send(&router, "GET", &path, None, &[]).await.0, 401);
        let (status, headers, bytes) = send(&router, "GET", &path, Some(&reader), &[]).await;
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&bytes));
        assert_eq!(headers["cache-control"], "no-store");
        assert!(
            headers["content-disposition"]
                .to_str()
                .unwrap()
                .starts_with("attachment;")
        );
        let text = String::from_utf8(bytes).unwrap();
        for expected in [
            "<Soup> & crème",
            "½ cup rice",
            "Prep: 0 min",
            "1 hr 30 min",
            "Mix <gently>",
        ] {
            // Markdown includes emphasis markers around the Prep label.
            if format == "markdown" && expected == "Prep: 0 min" {
                continue;
            }
            assert!(text.contains(expected), "{text}");
        }
        assert!(
            !text.contains("&lt;Soup&gt;"),
            "plain exports preserve literal text"
        );
        assert!(!text.contains("By \n"), "unknown authors must be omitted");
        let bulk = format!("/settings/export/{format}");
        assert_eq!(send(&router, "GET", &bulk, None, &[]).await.0, 401);
        let (status, headers, bytes) = send(&router, "GET", &bulk, Some(&reader), &[]).await;
        assert_eq!(status, 200);
        assert_eq!(headers["content-type"], "application/zip");
        let entries = zip_contents(&bytes);
        assert_eq!(entries.len(), 2);
        assert_ne!(
            entries[0]["name"], entries[1]["name"],
            "duplicate titles need distinct filenames"
        );
        for entry in entries {
            let name = entry["name"].as_str().unwrap();
            assert!(!name.contains('/') && !name.contains('\\') && !name.contains(".."));
            assert!(entry["text"].as_str().unwrap().contains("½ cup rice"));
        }
    }
    assert_eq!(
        send(
            &router,
            "GET",
            "/recipes/99999/export/text",
            Some(&reader),
            &[]
        )
        .await
        .0,
        404
    );
    assert_eq!(
        send(&router, "GET", "/settings/export/html", Some(&reader), &[])
            .await
            .0,
        400
    );
    assert_eq!(
        send(
            &router,
            "GET",
            &format!("/recipes/{}/print", catalog[0].id),
            None,
            &[]
        )
        .await
        .0,
        401
    );
    let (_, _, settings) = send(&router, "GET", "/exports", Some(&reader), &[]).await;
    let settings = String::from_utf8(settings).unwrap();
    assert!(settings.contains("/exports/download"));
    assert!(settings.contains("value=\"pdf\""));
    assert!(!settings.contains("/settings/templates"));
}

#[tokio::test]
async fn only_admin_can_change_templates_and_invalid_edits_are_retained_safely() {
    let (_temporary, app, admin_token, reader, catalog) = fixture();
    let saved = app.export_templates().unwrap().text;
    let router = web::router(app.clone());
    let csrf = "a".repeat(64);
    let custom = "Custom {{{title}}}: {{#steps}}{{number}}={{{text}}};{{/steps}}";
    let fields = [
        ("csrf", csrf.as_str()),
        ("format", "text"),
        ("source", custom),
    ];
    assert_eq!(
        send(
            &router,
            "POST",
            "/settings/templates",
            Some(&reader),
            &fields
        )
        .await
        .0,
        403
    );
    let (status, _, _) = send(
        &router,
        "POST",
        "/settings/templates",
        Some(&admin_token),
        &[("csrf", "wrong"), ("format", "text"), ("source", custom)],
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(app.export_templates().unwrap().text, saved);

    let invalid = "</textarea><script>alert(1)</script>{{#unclosed}}";
    let (status, _, body) = send(
        &router,
        "POST",
        "/settings/templates",
        Some(&admin_token),
        &[("csrf", &csrf), ("format", "text"), ("source", invalid)],
    )
    .await;
    assert_eq!(status, 400, "{}", String::from_utf8_lossy(&body));
    let html = String::from_utf8(body).unwrap();
    assert!(html.contains("{{#unclosed}}"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains("<script>alert(1)</script>"));
    assert_eq!(app.export_templates().unwrap().text, saved);
    let (status, headers, _) = send(
        &router,
        "POST",
        "/settings/templates",
        Some(&admin_token),
        &fields,
    )
    .await;
    assert_eq!(status, 303);
    assert_eq!(
        headers["location"],
        "/settings/templates?format=text&saved=1"
    );
    let (_, _, body) = send(
        &router,
        "GET",
        &format!("/recipes/{}/export/text", catalog[0].id),
        Some(&reader),
        &[],
    )
    .await;
    assert!(
        String::from_utf8(body)
            .unwrap()
            .starts_with("Custom <Soup> & crème: 1=Mix <gently>;2=Cook thoroughly;")
    );
}

#[test]
fn html_template_escapes_recipe_content_and_formats_times() {
    let (_temporary, app, _, _, catalog) = fixture();
    let html = mustache::render(
        &app.export_templates().unwrap().pdf,
        &exports::recipe_context(&catalog[0]).unwrap(),
    )
    .unwrap();
    for expected in [
        "&lt;Soup&gt; &amp; crème",
        "½ cup rice",
        "Mix &lt;gently&gt;",
        "Total: 1 hr 30 min",
        "Prep: 0 min",
    ] {
        assert!(html.contains(expected), "{html}");
    }
    assert!(!html.contains("<Soup>"));
    assert!(!html.contains("<p>By </p>"));
}

#[test]
fn empty_catalog_exports_a_valid_empty_zip() {
    let temporary = tempfile::tempdir().unwrap();
    let app = App::open(&temporary.path().join("db.sqlite")).unwrap();
    for format in ["text", "markdown", "pdf"] {
        let file = exports::export_all(&app, format).unwrap();
        assert!(zip_contents(&file.bytes).is_empty());
    }
}

#[tokio::test]
async fn printing_uses_inline_pdf_and_download_uses_attachment() {
    let (_temporary, app, admin_token, reader, catalog) = fixture();
    let admin = app.authenticate(&admin_token).unwrap().unwrap();
    app.save_export_template(
        &admin,
        "pdf",
        "<!doctype html><meta charset=utf-8><h1>{{title}}</h1><p>Configured PDF template</p>",
    )
    .unwrap();
    let router = web::router(app);
    let id = catalog[0].id;
    let (print_status, print_headers, print_bytes) = send(
        &router,
        "GET",
        &format!("/recipes/{id}/print"),
        Some(&reader),
        &[],
    )
    .await;
    let (download_status, download_headers, download_bytes) = send(
        &router,
        "GET",
        &format!("/recipes/{id}/export/pdf"),
        Some(&reader),
        &[],
    )
    .await;
    if print_status == 503 {
        assert!(
            std::env::var_os("REQUIRE_PDF_TESTS").is_none(),
            "PDF runtime is required in CI"
        );
        assert_eq!(download_status, 503);
        eprintln!("SKIP actual PDF response checks: WeasyPrint is unavailable");
        return;
    }
    assert_eq!(
        print_status,
        200,
        "{}",
        String::from_utf8_lossy(&print_bytes)
    );
    assert_eq!(download_status, 200);
    assert_eq!(print_headers["content-type"], "application/pdf");
    assert_eq!(download_headers["content-type"], "application/pdf");
    assert!(
        print_headers["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("inline;")
    );
    assert!(
        download_headers["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment;")
    );
    assert!(print_bytes.starts_with(b"%PDF-"));
    assert!(download_bytes.starts_with(b"%PDF-"));
    let temporary = tempfile::tempdir().unwrap();
    let pdf = temporary.path().join("print.pdf");
    std::fs::write(&pdf, &print_bytes).unwrap();
    if let Ok(text) = Command::new("pdftotext").arg(&pdf).arg("-").output() {
        assert!(text.status.success());
        let text = String::from_utf8(text.stdout).unwrap();
        assert!(text.contains("Configured PDF template"));
        assert!(text.contains("<Soup> & crème"));
    }
    let (status, _, bytes) = send(&router, "GET", "/settings/export/pdf", Some(&reader), &[]).await;
    assert_eq!(status, 200);
    let zip = temporary.path().join("pdfs.zip");
    std::fs::write(&zip, bytes).unwrap();
    let checked = Command::new("python3").args(["-c", "import sys,zipfile\nwith zipfile.ZipFile(sys.argv[1]) as z:\n assert z.testzip() is None\n assert len(z.namelist()) == 2\n assert all(n.endswith('.pdf') and z.read(n).startswith(b'%PDF-') for n in z.namelist())"]).arg(zip).output().unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

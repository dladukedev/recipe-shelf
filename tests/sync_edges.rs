use recipe_shelf::core::{App, Recipe, User};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, path::PathBuf};
use tempfile::TempDir;

fn setup() -> (TempDir, App, User) {
    let dir = tempfile::tempdir().unwrap();
    let app = App::open(&dir.path().join("recipes.sqlite")).unwrap();
    let admin = app.setup("admin", "correct horse battery staple").unwrap();
    (dir, app, admin)
}

fn recipe(app: &App, query: &str) -> Recipe {
    app.recipes(query, 100, 0)
        .unwrap()
        .into_iter()
        .find(|recipe| recipe.data.title == query)
        .unwrap()
}

fn put(root: &Path, relative: &str, contents: &str) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, contents).unwrap();
    path
}

#[test]
fn sibling_and_prefix_roots_are_isolated_during_pruning() {
    let (dir, app, admin) = setup();
    // `%` and `_` are literal path characters; these roots also share a textual prefix.
    let first = dir.path().join("recipes_%");
    let prefix_sibling = dir.path().join("recipes_%_extra");
    let wildcard_sibling = dir.path().join("recipes-X");
    for root in [&first, &prefix_sibling, &wildcard_sibling] {
        fs::create_dir_all(root).unwrap();
        app.add_folder(&admin, root.to_str().unwrap()).unwrap();
    }
    let first_file = put(
        &first,
        "nested/shared.toml",
        "title = \"Shared\"\ndescription = \"first root\"\n",
    );
    let prefix_file = put(
        &prefix_sibling,
        "nested/shared.toml",
        "title = \"Shared\"\ndescription = \"prefix sibling root\"\n",
    );
    let wildcard_file = put(
        &wildcard_sibling,
        "nested/shared.toml",
        "title = \"Shared\"\ndescription = \"wildcard sibling root\"\n",
    );

    let initial = app.sync(&admin).unwrap();
    assert_eq!(initial.added, 3);
    let prefix_id = app.recipes("prefix sibling root", 10, 0).unwrap()[0].id;
    let wildcard_id = app.recipes("wildcard sibling root", 10, 0).unwrap()[0].id;
    fs::remove_file(first_file).unwrap();
    let after_prune = app.sync(&admin).unwrap();
    assert_eq!(after_prune.removed, 1);
    let remaining = app.recipes("Shared", 10, 0).unwrap();
    assert_eq!(remaining.len(), 2);
    assert!(remaining.iter().any(|item| item.id == prefix_id));
    assert!(remaining.iter().any(|item| item.id == wildcard_id));
    assert!(prefix_file.exists());
    assert!(wildcard_file.exists());
}

#[test]
fn nested_duplicate_names_flatten_and_deleted_ids_are_not_reused() {
    let (dir, app, admin) = setup();
    let root = dir.path().join("recipes");
    fs::create_dir_all(&root).unwrap();
    app.add_folder(&admin, root.to_str().unwrap()).unwrap();
    put(
        &root,
        "one/dish.toml",
        "title = \"Duplicate\"\ndescription = \"one\"\n",
    );
    put(
        &root,
        "two/dish.toml",
        "title = \"Duplicate\"\ndescription = \"two\"\n",
    );
    assert_eq!(app.sync(&admin).unwrap().added, 2);
    let old = app.recipes("Duplicate", 10, 0).unwrap();
    assert_eq!(old.len(), 2);
    assert_ne!(old[0].id, old[1].id);

    fs::remove_file(root.join("one/dish.toml")).unwrap();
    assert_eq!(app.sync(&admin).unwrap().removed, 1);
    put(&root, "three/new.toml", "title = \"Replacement\"\n");
    assert_eq!(app.sync(&admin).unwrap().added, 1);
    let replacement = recipe(&app, "Replacement");
    assert!(replacement.id > old.iter().map(|item| item.id).max().unwrap());
}

#[cfg(unix)]
#[test]
fn sync_does_not_follow_file_or_directory_symlinks() {
    use std::os::unix::fs::symlink;

    let (dir, app, admin) = setup();
    let root = dir.path().join("recipes");
    let outside = dir.path().join("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    app.add_folder(&admin, root.to_str().unwrap()).unwrap();
    put(&outside, "external.toml", "title = \"External\"\n");
    symlink(outside.join("external.toml"), root.join("linked.toml")).unwrap();
    symlink(&outside, root.join("linked-dir")).unwrap();

    let report = app.sync(&admin).unwrap();
    assert_eq!(report.added, 0);
    assert!(app.recipes("External", 10, 0).unwrap().is_empty());
}

#[test]
fn invalid_and_oversized_files_are_reported_without_losing_last_good_recipe() {
    let (dir, app, admin) = setup();
    let root = dir.path().join("recipes");
    fs::create_dir_all(&root).unwrap();
    app.add_folder(&admin, root.to_str().unwrap()).unwrap();
    let existing = put(
        &root,
        "valid.toml",
        "title = \"Good\"\ndescription = \"kept\"\n",
    );
    assert_eq!(app.sync(&admin).unwrap().added, 1);
    let original = recipe(&app, "Good");

    fs::write(&existing, "description = \"missing title\"\n").unwrap();
    put(
        &root,
        "large.toml",
        &format!("title = \"Too large\"\n#{}", "x".repeat(1024 * 1024)),
    );
    let report = app.sync(&admin).unwrap();
    assert_eq!(report.errors.len(), 2);
    assert_eq!(report.removed, 0);
    assert_eq!(
        app.recipe(original.id).unwrap().unwrap().raw_toml,
        original.raw_toml
    );
    assert!(app.recipes("Too large", 10, 0).unwrap().is_empty());
}

#[test]
fn indexed_recipes_and_sessions_survive_reopen_but_expired_tokens_do_not_authenticate() {
    let (dir, app, admin) = setup();
    let root = dir.path().join("recipes");
    fs::create_dir_all(&root).unwrap();
    app.add_folder(&admin, root.to_str().unwrap()).unwrap();
    put(&root, "nested/persist.toml", "title = \"Persisted\"\n");
    app.sync(&admin).unwrap();
    let login = app.login("admin", "correct horse battery staple").unwrap();
    drop(app);

    let reopened = App::open(&dir.path().join("recipes.sqlite")).unwrap();
    assert_eq!(recipe(&reopened, "Persisted").data.title, "Persisted");
    assert!(reopened.authenticate(&login.token).unwrap().is_some());

    let digest = hex::encode(Sha256::digest(login.token.as_bytes()));
    let connection = Connection::open(dir.path().join("recipes.sqlite")).unwrap();
    connection
        .execute(
            "UPDATE sessions SET expires_at=0 WHERE token_hash=?1",
            [&digest],
        )
        .unwrap();
    assert!(reopened.authenticate(&login.token).unwrap().is_none());
}

use super::*;
use super::{parsing::read_recipe, sessions::token_hash};
use rusqlite::params;
use tempfile::tempdir;
fn app() -> (tempfile::TempDir, App, User) {
    let d = tempdir().unwrap();
    let a = App::open(&d.path().join("db.sqlite")).unwrap();
    let admin = a.setup("admin", "this is a very long password").unwrap();
    (d, a, admin)
}
#[test]
fn setup_is_atomic_and_password_rules_apply() {
    let (_d, a, u) = app();
    assert!(u.is_admin);
    assert!(!a.needs_setup().unwrap());
    assert!(a.setup("second", "this is a very long password").is_err());
    assert!(a.create_user(&u, "x", "short", false).is_err());
    assert!(
        a.create_user(&u, "", "this is a very long password", false)
            .is_err()
    );
}
#[test]
fn eight_character_passwords_work_across_account_operations() {
    let d = tempdir().unwrap();
    let a = App::open(&d.path().join("db.sqlite")).unwrap();
    assert!(a.setup("admin", "1234567").is_err());
    let admin = a.setup("admin", "12345678").unwrap();
    assert!(a.login("admin", "12345678").is_ok());
    assert!(a.create_user(&admin, "reader", "1234567", false).is_err());
    let reader = a.create_user(&admin, "reader", "abcdefgh", false).unwrap();
    assert!(a.login("reader", "abcdefgh").is_ok());
    assert!(
        a.update_user(&admin, reader.id, "reader", Some("1234567"), false)
            .is_err()
    );
    a.update_user(&admin, reader.id, "reader", Some("87654321"), false)
        .unwrap();
    assert!(a.login("reader", "87654321").is_ok());
    assert!(a.change_password(&reader, "87654321", "1234567").is_err());
    a.change_password(&reader, "87654321", "abcdefgh").unwrap();
    assert!(a.login("reader", "abcdefgh").is_ok());
}
#[test]
fn concurrent_first_setup_creates_exactly_one_admin() {
    let d = tempdir().unwrap();
    let a = App::open(&d.path().join("race.sqlite")).unwrap();
    let b = a.clone();
    let t1 = std::thread::spawn(move || a.setup("first", "this is a very long password"));
    let t2 = std::thread::spawn(move || b.setup("second", "this is a very long password"));
    let results = [t1.join().unwrap(), t2.join().unwrap()];
    assert_eq!(results.iter().filter(|x| x.is_ok()).count(), 1);
    assert_eq!(results.iter().filter(|x| x.is_err()).count(), 1);
}
#[test]
fn sessions_are_opaque_and_revocable() {
    let (_d, a, _) = app();
    let l = a.login("ADMIN", "this is a very long password").unwrap();
    assert_eq!(l.token.len(), 64);
    assert_eq!(a.authenticate(&l.token).unwrap().unwrap().username, "admin");
    a.logout(&l.token).unwrap();
    assert!(a.authenticate(&l.token).unwrap().is_none());
    assert!(a.login("admin", "wrong password").is_err());
}
#[test]
fn sessions_store_only_hashes_expire_and_are_cleaned_on_login() {
    let (_d, a, _) = app();
    let first = a.login("admin", "this is a very long password").unwrap();
    {
        let db = a.conn().unwrap();
        let stored: String = db
            .query_row("SELECT token_hash FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored, token_hash(&first.token));
        assert_ne!(stored, first.token);
        db.execute("UPDATE sessions SET expires_at=0", []).unwrap();
    }
    assert!(a.authenticate(&first.token).unwrap().is_none());
    let _second = a.login("admin", "this is a very long password").unwrap();
    let count: i64 = a
        .conn()
        .unwrap()
        .query_row("SELECT count(*) FROM sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}
#[test]
fn recipe_times_read_legacy_strings_and_new_integer_minutes() {
    let legacy: RecipeData = serde_json::from_str(
        r#"{"title":"Old","prep_time":"PT1H40M","cook_time":"PT0M","total_time":""}"#,
    )
    .unwrap();
    assert_eq!(legacy.prep_time, Some(100));
    assert_eq!(legacy.cook_time, Some(0));
    assert_eq!(legacy.total_time, None);
    let encoded = serde_json::to_value(&legacy).unwrap();
    assert_eq!(encoded["prep_time"], 100);
    assert_eq!(encoded["cook_time"], 0);
    assert!(encoded.get("total_time").is_none());

    let dir = tempdir().unwrap();
    let path = dir.path().join("recipe.toml");
    std::fs::write(
        &path,
        "title = \"Times\"\nprep_time = 100\ncook_time = \"PT1H40M\"\ntotal_time = \"\"\n",
    )
    .unwrap();
    let (data, _) = read_recipe(&path).unwrap();
    assert_eq!(data.prep_time, Some(100));
    assert_eq!(data.cook_time, Some(100));
    assert_eq!(data.total_time, None);
}
#[test]
fn cached_legacy_recipe_json_remains_readable_before_sync() {
    let (_d, a, _) = app();
    let db = a.conn().unwrap();
    db.execute("INSERT INTO folders(path) VALUES('/cached')", [])
        .unwrap();
    let folder_id = db.last_insert_rowid();
    db.execute(
            "INSERT INTO recipes(folder_id,path,title,raw_toml,data_json) VALUES(?1,'/cached/old.toml','Old','title = \"Old\"',?2)",
            params![folder_id, r#"{"title":"Old","prep_time":"PT1H40M","cook_time":"","total_time":"PT2H"}"#],
        )
        .unwrap();
    drop(db);
    let cached = a.recipe(1).unwrap().unwrap();
    assert_eq!(cached.data.prep_time, Some(100));
    assert_eq!(cached.data.cook_time, None);
    assert_eq!(cached.data.total_time, Some(120));
}
#[test]
fn recipe_times_reject_negative_fractional_and_malformed_values() {
    for json in [
        r#"{"prep_time":-1}"#,
        r#"{"prep_time":1.5}"#,
        r#"{"prep_time":"PT1M30S"}"#,
        r#"{"prep_time":"PT1H2H"}"#,
    ] {
        assert!(serde_json::from_str::<RecipeData>(json).is_err(), "{json}");
    }
}
#[test]
fn export_templates_use_defaults_and_persist_admin_changes() {
    let (_d, a, admin) = app();
    let reader = a
        .create_user(&admin, "reader", "another long password", false)
        .unwrap();
    let defaults = a.export_templates().unwrap();
    assert!(!defaults.text.is_empty());
    assert!(a.save_export_template(&reader, "text", "changed").is_err());
    assert!(a.save_export_template(&admin, "html", "changed").is_err());
    assert!(
        a.save_export_template(&admin, "text", "{{#broken}}")
            .is_err()
    );
    a.save_export_template(&admin, "text", "{{{title}}}")
        .unwrap();
    assert_eq!(a.export_templates().unwrap().text, "{{{title}}}");
    assert_eq!(a.export_templates().unwrap().markdown, defaults.markdown);
}
#[test]
fn export_recipes_is_not_limited_to_browse_page_size() {
    let (_d, a, _) = app();
    let db = a.conn().unwrap();
    db.execute("INSERT INTO folders(path) VALUES('/bulk')", [])
        .unwrap();
    let folder_id = db.last_insert_rowid();
    for i in 0..501 {
        db.execute(
                "INSERT INTO recipes(folder_id,path,title,raw_toml,data_json) VALUES(?1,?2,?3,'title = \"Bulk\"','{\"title\":\"Bulk\"}')",
                params![folder_id, format!("/bulk/{i}.toml"), format!("Bulk {i:03}")],
            )
            .unwrap();
    }
    drop(db);
    assert_eq!(a.recipes("", 10_000, 0).unwrap().len(), 500);
    assert_eq!(a.export_recipes().unwrap().len(), 501);
}
#[test]
fn admin_checks_and_folder_overlap() {
    let (d, a, admin) = app();
    let ordinary = a
        .create_user(&admin, "reader", "another long password", false)
        .unwrap();
    assert!(a.users(&ordinary).is_err());
    let root = d.path().join("recipes");
    std::fs::create_dir(&root).unwrap();
    let sub = root.join("sub");
    std::fs::create_dir(&sub).unwrap();
    a.add_folder(&admin, root.to_str().unwrap()).unwrap();
    assert!(a.add_folder(&admin, sub.to_str().unwrap()).is_err());
    assert!(a.add_folder(&ordinary, root.to_str().unwrap()).is_err());
}
#[test]
fn sync_updates_preserves_bad_and_prunes_removed_searches_unicode() {
    let (d, a, admin) = app();
    let root = d.path().join("recipes");
    std::fs::create_dir(&root).unwrap();
    let file = root.join("one.toml");
    std::fs::write(
        &file,
        "title = \"Crème brûlée\"\ndescription = \"café %_ search\"\n",
    )
    .unwrap();
    a.add_folder(&admin, root.to_str().unwrap()).unwrap();
    assert_eq!(a.sync(&admin).unwrap().added, 1);
    let id = a.recipes("CAFÉ %_", 10, 0).unwrap()[0].id;
    std::fs::write(&file, "not valid [").unwrap();
    let rep = a.sync(&admin).unwrap();
    assert_eq!(rep.errors.len(), 1);
    assert_eq!(a.recipe(id).unwrap().unwrap().data.title, "Crème brûlée");
    std::fs::write(&file, "title = \"Crème dessert\"\n").unwrap();
    assert_eq!(a.sync(&admin).unwrap().updated, 1);
    assert_eq!(a.recipe(id).unwrap().unwrap().data.title, "Crème dessert");
    std::fs::remove_file(&file).unwrap();
    assert_eq!(a.sync(&admin).unwrap().removed, 1);
    assert!(a.recipe(id).unwrap().is_none());
}
#[test]
fn missing_root_does_not_prune_index() {
    let (d, a, admin) = app();
    let root = d.path().join("recipes");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("x.toml"), "title = \"x\"").unwrap();
    let folder = a.add_folder(&admin, root.to_str().unwrap()).unwrap();
    a.sync(&admin).unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(a.sync(&admin).unwrap().removed, 0);
    assert_eq!(a.recipes("", 10, 0).unwrap().len(), 1);
    assert_eq!(a.folders(&admin).unwrap()[0].id, folder.id);
}

#[cfg(unix)]
#[test]
fn sync_skips_symlinked_folder_root() {
    use std::os::unix::fs::symlink;
    let (d, a, admin) = app();
    let target = d.path().join("actual-recipes");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("x.toml"), "title = \"x\"").unwrap();
    let link = d.path().join("recipes-link");
    symlink(&target, &link).unwrap();
    let folder = a.add_folder(&admin, link.to_str().unwrap()).unwrap();
    // add_folder canonicalizes roots, so replace its path to exercise a
    // symlink created after registration.
    std::fs::remove_dir_all(&target).unwrap();
    symlink(d.path().join("absent"), &target).unwrap();
    let report = a.sync(&admin).unwrap();
    assert_eq!(report.added, 0);
    assert!(report.errors.iter().any(|e| e.contains("unavailable")));
    assert_eq!(a.folders(&admin).unwrap()[0].id, folder.id);
}

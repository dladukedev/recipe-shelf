use super::*;

#[page("/settings/folders")]
pub(in crate::web) async fn folders_settings(cx: &Cx) -> topcoat::Result<impl View> {
    let user = require_web_admin(cx).await?;
    let actor = user.clone();
    let folders = db(cx, move |a| a.folders(&actor))
        .await
        .map_err(server_error)?;
    Ok(view! {
        cx =>
        settings_frame(
            section: SettingsSection::Folders,
            user: &user,
            <section class="panel">
                <h2>"Recipe folders"</h2>
                <p class="muted">
                    "Read-only TOML files from your mounted folders. Removing a folder removes its recipes from this shelf; source files are kept."
                </p>
                <ul class="folder-list">
                    if folders.is_empty() {
                        <li class="muted">
                            "No folders connected. Add a folder to get started."
                        </li>
                    }
                    for folder in &folders {
                        <li>
                            <div class="folder-info">
                                <code>(folder.path.as_str())</code>
                                <p class="muted">
                                    (folder.recipe_count)
                                    " recipes"
                                    if let Some(result) = &folder.last_sync {
                                        " · "
                                        (sync_age(result.completed_at))
                                    }
                                </p>
                                if let Some(result) = &folder.last_sync {
                                    <p class="muted">
                                        (result.added)
                                        " added, "
                                        (result.updated)
                                        " updated, "
                                        (result.removed)
                                        " removed"
                                    </p>
                                    if !result.errors.is_empty() {
                                        <ul class="error-list">
                                            for error in &result.errors {
                                                <li>(error.as_str())</li>
                                            }
                                        </ul>
                                    } else {
                                        <p class="sync-success">"Sync complete"</p>
                                    }
                                } else {
                                    <p class="muted">"Not synced yet"</p>
                                }
                            </div>
                            <form
                                method="post"
                                action=(href!(crate::web::settings::folders::remove_folder))
                                data-confirm="Remove this folder and its recipes from the shelf? Source files will be kept."
                            >
                                csrf_field()
                                <input
                                    type="hidden"
                                    name="id"
                                    value=(folder.id.to_string())
                                />
                                <button
                                    class="button button-secondary danger"
                                    type="submit"
                                    aria-label=(format!("Remove {}", folder.path))
                                >
                                    "Remove"
                                </button>
                            </form>
                        </li>
                    }
                </ul>
                <form
                    method="post"
                    action=(href!(crate::web::settings::folders::add_folder))
                    class="inline-form"
                >
                    csrf_field()
                    <label class="sr-only" for="folder-path">
                        "Mounted folder path"
                    </label>
                    <input
                        id="folder-path"
                        name="path"
                        required=(true)
                        placeholder="/recipes/new-folder"
                    />
                    <button class="button button-secondary" type="submit">
                        "Add folder"
                    </button>
                </form>
                <form
                    method="post"
                    action=(href!(crate::web::settings::folders::sync))
                    class="sync-form"
                >
                    csrf_field()
                    <button class="button button-primary" type="submit">
                        "↻ Sync recipes"
                    </button>
                    <span class="muted">"Source files stay read-only."</span>
                </form>
            </section>
        )
    })
}

#[derive(Deserialize)]
pub(in crate::web) struct FolderForm {
    csrf: String,
    path: String,
}
#[page(POST "/settings/folders")]
pub(in crate::web) async fn add_folder(
    cx: &Cx,
    Form(form): Form<FolderForm>,
) -> topcoat::Result<()> {
    require_csrf(cx, &form.csrf)?;
    let user = require_admin(cx).await?;
    let path = form.path;
    if !accepted(db(cx, move |a| a.add_folder(&user, &path)).await)? {
        return Err(see_other(
            href!(crate::web::settings::folders::folders_settings)
                .query([("error", "folder")])
                .resolve(cx),
        )
        .into());
    }
    Err(see_other(
        href!(crate::web::settings::folders::folders_settings)
            .query([("saved", "1")])
            .resolve(cx),
    )
    .into())
}
#[page(POST "/settings/sync")]
pub(in crate::web) async fn sync(cx: &Cx, Form(form): Form<CsrfOnly>) -> topcoat::Result<()> {
    require_csrf(cx, &form.csrf)?;
    let user = require_admin(cx).await?;
    db(cx, move |app| app.sync(&user))
        .await
        .map_err(server_error)?;
    Err(see_other(href!(folders_settings).query([("synced", "1")]).resolve(cx)).into())
}

#[page(POST "/settings/folders/remove")]
pub(in crate::web) async fn remove_folder(
    cx: &Cx,
    Form(form): Form<IdForm>,
) -> topcoat::Result<()> {
    require_csrf(cx, &form.csrf)?;
    let actor = require_admin(cx).await?;
    match accepted(db(cx, move |a| a.remove_folder(&actor, form.id)).await)? {
        true => Err(see_other(
            href!(crate::web::settings::folders::folders_settings)
                .query([("saved", "1")])
                .resolve(cx),
        )
        .into()),
        false => Err(see_other(
            href!(crate::web::settings::folders::folders_settings)
                .query([("error", "folder")])
                .resolve(cx),
        )
        .into()),
    }
}

pub(super) fn sync_age(completed_at: i64) -> String {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(completed_at.max(0) as u64);
    if elapsed < 60 {
        "Synced just now".into()
    } else if elapsed < 3600 {
        format!("Synced {} min ago", elapsed / 60)
    } else {
        format!("Synced {} hr ago", elapsed / 3600)
    }
}

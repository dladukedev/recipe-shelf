use super::*;

#[page("/settings/users")]
pub(in crate::web) async fn users_settings(cx: &Cx) -> topcoat::Result<impl View> {
    let user = require_web_admin(cx).await?;
    let actor = user.clone();
    let people = db(cx, move |a| a.users(&actor))
        .await
        .map_err(server_error)?;
    let selection = &topcoat::router::query_params::<SelectionQuery>(cx)?
        .fields
        .0;
    let edit_id = i64::try_from(number(selection.edit.as_deref(), "edit", 0)?)
        .map_err(|_| bad_request("edit is out of range"))?;
    let editing = people.iter().find(|person| person.id == edit_id).cloned();
    let feedback = &topcoat::router::query_params::<FeedbackQuery>(cx)?.fields.0;
    let user_error = feedback.error.as_deref() == Some("user");
    let user_open = editing.is_some() || selection.add.is_some() || user_error;

    if edit_id != 0 && editing.is_none() {
        return Err(not_found().into());
    }
    Ok(view! {
        cx =>
        settings_frame(
            section: SettingsSection::Users,
            user: &user,
            <div class="users-layout">
                <section class="panel">
                    <div class="panel-toolbar">
                        <h2>
                            "Users "
                            <span class="tag">(people.len())</span>
                        </h2>
                        <a
                            class="button button-primary"
                            href=(href!(users_settings).query([("add", "1")]))
                            data-modal-link="user-dialog"
                        >
                            "Add user"
                        </a>
                    </div>
                    <div class="people-list">
                        for person in &people {
                            <div class="person">
                                <div>
                                    <strong>
                                        (person.username.as_str())
                                        if person.id == user.id {
                                            " (you)"
                                        }
                                    </strong>
                                    <p class="muted">
                                        (if person.is_admin { "Administrator" } else { "Member" })
                                    </p>
                                </div>
                                <div class="button-row">
                                    <a
                                        class="button button-secondary"
                                        data-modal-link="user-dialog"
                                        href=(href!(crate::web::settings::users::users_settings).query(
                                            [("edit", person.id)],
                                        ))
                                    >
                                        "Edit"
                                    </a>
                                    <form
                                        method="post"
                                        action=(href!(crate::web::settings::users::delete_user))
                                        data-confirm=(format!(
                                            "Delete the account for {}?",
                                            person.username,
                                        ))
                                    >
                                        csrf_field()
                                        <input
                                            type="hidden"
                                            name="id"
                                            value=(person.id.to_string())
                                        />
                                        <button
                                            class="button button-secondary danger"
                                            type="submit"
                                            disabled=(person.id == user.id)
                                        >
                                            "Delete"
                                        </button>
                                    </form>
                                </div>
                            </div>
                        }
                    </div>
                </section>
                <dialog
                    id="user-dialog"
                    class="modal"
                    data-modal=(true)
                    open=(user_open)
                    aria-labelledby="user-dialog-title"
                >
                    <div class="modal-header">
                        <h2 id="user-dialog-title">
                            (if editing.is_some() { "Update user" } else { "Add user" })
                        </h2>
                        <a
                            class="icon-button"
                            href=(href!(users_settings))
                            data-modal-close=(true)
                            aria-label="Close user dialog"
                        >
                            "×"
                        </a>
                    </div>
                    if user_error {
                        <p class="notice notice-error" role="alert">
                            "The account could not be changed. Use a unique username, a password of 8–1024 bytes, and keep at least one administrator."
                        </p>
                    }
                    <form
                        class="stack-form"
                        method="post"
                        action=(if editing.is_some() {
                            href!(crate::web::settings::users::update_user).resolve(cx)
                        } else {
                            href!(crate::web::settings::users::create_user).resolve(cx)
                        })
                    >
                        csrf_field()
                        if let Some(person) = &editing {
                            <input
                                type="hidden"
                                name="id"
                                value=(person.id.to_string())
                            />
                        }
                        <label for="user-name">"Username"</label>
                        <input
                            id="user-name"
                            name="username"
                            required=(true)
                            maxlength="64"
                            autocomplete="off"
                            value=(editing
                                .as_ref()
                                .map(|p| p.username.as_str())
                                .unwrap_or(""))
                        />
                        if editing.as_ref().is_none_or(|p| p.id != user.id) {
                            <label for="user-password">
                                (if editing.is_some() {
                                    "Reset password (optional)"
                                } else {
                                    "Password"
                                })
                            </label>
                            <input
                                id="user-password"
                                name="password"
                                type="password"
                                minlength="8"
                                maxlength="1024"
                                required=(editing.is_none())
                                autocomplete="new-password"
                            />
                            <p class="muted form-hint">
                                (if editing.is_some() {
                                    "Leave blank to keep the password. A reset signs out this user’s sessions."
                                } else {
                                    "At least 8 characters."
                                })
                            </p>
                        } else {
                            <p class="muted">
                                "Change your password in "
                                <a
                                    class="text-link"
                                    href=(href!(crate::web::settings::basic::settings))
                                >
                                    "Basic settings"
                                </a>
                                "."
                            </p>
                        }
                        <label class="check-row">
                            <input
                                name="is_admin"
                                type="checkbox"
                                value="true"
                                checked=(editing.as_ref().is_some_and(|p| p.is_admin))
                                disabled=(editing.as_ref().is_some_and(|p| p.id == user.id))
                            />
                            "Administrator access"
                        </label>
                        if editing.as_ref().is_some_and(|p| p.id == user.id) {
                            <input type="hidden" name="is_admin" value="true" />
                        }
                        <div class="button-row">
                            <button class="button button-primary" type="submit">
                                (if editing.is_some() {
                                    "Save changes"
                                } else {
                                    "Add user"
                                })
                            </button>
                            <a
                                class="button button-secondary"
                                href=(href!(users_settings))
                                data-modal-close=(true)
                            >
                                "Cancel"
                            </a>
                        </div>
                    </form>
                </dialog>
            </div>
        )
    })
}

#[derive(Deserialize)]
pub(in crate::web) struct NewUser {
    csrf: String,
    username: String,
    password: String,
    is_admin: Option<bool>,
}
#[page(POST "/settings/users")]
pub(in crate::web) async fn create_user(cx: &Cx, Form(form): Form<NewUser>) -> topcoat::Result<()> {
    require_csrf(cx, &form.csrf)?;
    let actor = require_admin(cx).await?;
    let NewUser {
        username,
        password,
        is_admin,
        ..
    } = form;
    if !accepted(
        db(cx, move |a| {
            a.create_user(&actor, &username, &password, is_admin.unwrap_or(false))
        })
        .await,
    )? {
        return Err(see_other(
            href!(crate::web::settings::users::users_settings)
                .query([("error", "user")])
                .resolve(cx),
        )
        .into());
    }
    Err(see_other(
        href!(crate::web::settings::users::users_settings)
            .query([("saved", "1")])
            .resolve(cx),
    )
    .into())
}

#[derive(Deserialize)]
pub(in crate::web) struct UpdateUserForm {
    csrf: String,
    id: i64,
    username: String,
    password: Option<String>,
    is_admin: Option<bool>,
}
#[page(POST "/settings/users/update")]
pub(in crate::web) async fn update_user(
    cx: &Cx,
    Form(form): Form<UpdateUserForm>,
) -> topcoat::Result<()> {
    require_csrf(cx, &form.csrf)?;
    let actor = require_admin(cx).await?;
    let id = form.id;
    let result = db(cx, move |a| {
        a.update_user(
            &actor,
            id,
            &form.username,
            form.password.as_deref().filter(|p| !p.is_empty()),
            form.is_admin.unwrap_or(false),
        )
    })
    .await;
    Err(see_other(if accepted(result)? {
        "/settings/users?saved=1".to_owned()
    } else {
        href!(users_settings)
            .query([("edit", id.to_string()), ("error", "user".to_owned())])
            .resolve(cx)
    })
    .into())
}
#[page(POST "/settings/users/delete")]
pub(in crate::web) async fn delete_user(cx: &Cx, Form(form): Form<IdForm>) -> topcoat::Result<()> {
    require_csrf(cx, &form.csrf)?;
    let actor = require_admin(cx).await?;
    let result = db(cx, move |a| a.delete_user(&actor, form.id)).await;
    Err(see_other(if accepted(result)? {
        "/settings/users?saved=1"
    } else {
        "/settings/users?error=user"
    })
    .into())
}

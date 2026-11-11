use super::*;

#[page("/settings")]
pub(in crate::web) async fn settings(cx: &Cx) -> topcoat::Result<impl View> {
    let user = require_web_user(cx).await?;
    let feedback = &topcoat::router::query_params::<FeedbackQuery>(cx)?.fields.0;
    let selection = &topcoat::router::query_params::<SelectionQuery>(cx)?
        .fields
        .0;
    let password_error = match feedback.error.as_deref() {
        Some("password-match") => Some("The new passwords do not match."),
        Some("password") => Some(
            "Password not changed. Check your current password and choose a different new password of 8–1024 bytes.",
        ),
        _ => None,
    };
    let password_open = selection.modal == "password" || password_error.is_some();

    Ok(view! {
        cx =>
        settings_frame(
            section: SettingsSection::Basic,
            user: &user,
            <div class="settings-stack">
                <section class="panel appearance-settings">
                    <div class="settings-row">
                        <div>
                            <h2>"Appearance"</h2>
                            <p class="muted">"Choose a theme or follow your device."</p>
                        </div>
                        <div class="theme-options" role="group" aria-label="Color theme">
                            <button type="button" data-theme-choice="light">"Light"</button>
                            <button type="button" data-theme-choice="dark">"Dark"</button>
                            <button type="button" data-theme-choice="system">"System"</button>
                        </div>
                    </div>
                    <label class="check-row custom-theme-toggle" for="custom-theme-toggle">
                        <input
                            id="custom-theme-toggle"
                            type="checkbox"
                            role="switch"
                            data-custom-theme-toggle=(true)
                            aria-controls="custom-theme-editor"
                            aria-describedby="custom-theme-help"
                        />
                        "Custom Theme"
                    </label>
                    <p class="muted form-hint" id="custom-theme-help">
                        "Choose your own colors for light and dark mode. Colors save automatically in this browser."
                    </p>
                    <div id="custom-theme-editor" class="custom-theme-editor" hidden=(true) data-custom-theme-editor=(true)>
                        <p class="muted form-hint custom-theme-format">"Use a color picker or enter a six-digit hex color, such as #2563eb."</p>
                        <div class="custom-theme-palettes">
                            for (mode, heading, colors) in [
                                ("light", "Light theme", [
                                    ("foreground", "Foreground", "#111111"),
                                    ("background", "Background", "#ffffff"),
                                    ("accent", "Accent", "#2563eb"),
                                    ("danger", "Danger", "#b42318"),
                                ]),
                                ("dark", "Dark theme", [
                                    ("foreground", "Foreground", "#f2f2f2"),
                                    ("background", "Background", "#161616"),
                                    ("accent", "Accent", "#85adff"),
                                    ("danger", "Danger", "#ff9b91"),
                                ]),
                            ] {
                                <fieldset class="custom-theme-palette">
                                    <legend>(heading)</legend>
                                    for (name, label, value) in colors {
                                        <div class="custom-theme-field">
                                            <label for=(format!("theme-{mode}-{name}"))>(label)</label>
                                            <div class="custom-theme-color">
                                                <input
                                                    type="color"
                                                    value=(value)
                                                    aria-label=(format!("{heading} {label} color picker"))
                                                    data-custom-theme-mode=(mode)
                                                    data-custom-theme-color=(name)
                                                />
                                                <input
                                                    id=(format!("theme-{mode}-{name}"))
                                                    type="text"
                                                    value=(value)
                                                    pattern="#[0-9a-fA-F]{6}"
                                                    maxlength="7"
                                                    spellcheck="false"
                                                    autocomplete="off"
                                                    required=(true)
                                                    aria-label=(format!("{heading} {label} hex color"))
                                                    data-custom-theme-mode=(mode)
                                                    data-custom-theme-color=(name)
                                                />
                                            </div>
                                        </div>
                                    }
                                </fieldset>
                            }
                        </div>
                        <button type="button" class="button button-secondary" data-custom-theme-reset=(true)>
                            "Reset colors"
                        </button>
                    </div>
                </section>
                <section class="panel settings-row">
                    <div>
                        <h2>"Your account"</h2>
                        <p class="muted">
                            (user.username.as_str())
                            " · "
                            (if user.is_admin { "Administrator" } else { "Member" })
                        </p>
                    </div>
                    <span class="tag">
                        (if user.is_admin {
                            "Full collection access"
                        } else {
                            "Recipe browsing & exports"
                        })
                    </span>
                </section>
                <section class="panel settings-row">
                    <div>
                        <h2>"Password"</h2>
                        <p class="muted">"Manage your sign-in password."</p>
                    </div>
                    <a
                        class="button button-secondary"
                        href=(href!(settings).query([("modal", "password")]))
                        data-modal-link="password-dialog"
                    >
                        "Change password"
                    </a>
                </section>
                <dialog
                    id="password-dialog"
                    class="modal"
                    data-modal=(true)
                    open=(password_open)
                    aria-labelledby="password-dialog-title"
                >
                    <div class="modal-header">
                        <h2 id="password-dialog-title">"Change your password"</h2>
                        <a
                            href=(href!(settings))
                            data-modal-close=(true)
                            class="icon-button"
                            aria-label="Close password dialog"
                        >
                            "×"
                        </a>
                    </div>
                    if let Some(message) = password_error {
                        <p class="notice notice-error" role="alert">(message)</p>
                    }
                    <p class="muted">
                        "Changing your password signs out all your sessions. Sign in again with your new password."
                    </p>
                    <form
                        class="stack-form password-form"
                        method="post"
                        action=(href!(crate::web::settings::basic::change_password))
                        data-password-form=(true)
                    >
                        csrf_field()
                        <input
                            type="hidden"
                            name="username"
                            value=(user.username.as_str())
                            autocomplete="username"
                        />
                        <label for="current-password">"Current password"</label>
                        <input
                            id="current-password"
                            name="current"
                            type="password"
                            autocomplete="current-password"
                            required=(true)
                            maxlength="1024"
                        />
                        <label for="new-password">"New password"</label>
                        <input
                            id="new-password"
                            name="password"
                            type="password"
                            autocomplete="new-password"
                            required=(true)
                            minlength="8"
                            maxlength="1024"
                        />
                        <label for="confirm-password">"Confirm new password"</label>
                        <input
                            id="confirm-password"
                            name="confirm"
                            type="password"
                            autocomplete="new-password"
                            required=(true)
                            minlength="8"
                            maxlength="1024"
                        />
                        <button class="button button-primary" type="submit">
                            "Update password"
                        </button>
                    </form>
                    <a
                        class="text-link modal-cancel"
                        href=(href!(settings))
                        data-modal-close=(true)
                    >
                        "Cancel"
                    </a>
                </dialog>
            </div>
        )
    })
}

#[derive(Deserialize)]
pub(in crate::web) struct PasswordForm {
    csrf: String,
    current: String,
    password: String,
    confirm: String,
}
#[page(POST "/settings/password")]
pub(in crate::web) async fn change_password(
    cx: &Cx,
    Form(form): Form<PasswordForm>,
) -> topcoat::Result<()> {
    require_csrf(cx, &form.csrf)?;
    let actor = web_user(cx).await?;
    if form.password != form.confirm {
        return Err(see_other(
            href!(crate::web::settings::basic::settings)
                .query([("error", "password-match")])
                .resolve(cx),
        )
        .into());
    }
    if !accepted(
        db(cx, move |a| {
            a.change_password(&actor, &form.current, &form.password)
        })
        .await,
    )? {
        return Err(see_other(
            href!(crate::web::settings::basic::settings)
                .query([("error", "password")])
                .resolve(cx),
        )
        .into());
    }
    clear_session(cx);
    Err(see_other(
        href!(crate::web::login::login_get)
            .query([("password", "changed")])
            .resolve(cx),
    )
    .into())
}

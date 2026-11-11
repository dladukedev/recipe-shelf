use super::*;

#[derive(Deserialize)]
pub(super) struct Credentials {
    username: String,
    password: String,
    csrf: String,
}
pub(super) fn login_form<'a>(
    cx: &'a Cx,
    title: &'a str,
    heading: &'a str,
    message: &'a str,
    action: &'a str,
) -> topcoat::Result<impl View + 'a> {
    let feedback = &topcoat::router::query_params::<FeedbackQuery>(cx)?.fields.0;
    let error = &feedback.error;
    let created = feedback.created.is_some();
    Ok(shell(
        cx,
        title.to_owned(),
        Child::new(view! {
            cx =>
            <section class="narrow-card auth-card">
                if action == "/setup" {
                    <p class="eyebrow">"Recipe Shelf"</p>
                }
                <h1>(heading)</h1>
                if !message.is_empty() {
                    <p class="muted">(message)</p>
                }
                if feedback.password.as_deref() == Some("changed") {
                    snackbar(
                        message: "Password changed. Sign in with your new password."
                    )
                }
                if created {
                    snackbar(
                        message: "Administrator account created. Sign in to continue."
                    )
                }
                if error.as_deref() == Some("invalid") {
                    <p class="notice notice-error" role="alert">
                        "Use a unique username and a password of at least 8 characters."
                    </p>
                }
                if error.as_deref() == Some("credentials") {
                    <p class="notice notice-error" role="alert">
                        "The username or password is incorrect. Please try again."
                    </p>
                }
                if error.as_deref() == Some("throttled") {
                    <p class="notice notice-error" role="alert">
                        "Too many sign-in attempts. Wait five minutes and try again."
                    </p>
                }
                <form method="post" action=(action) class="stack-form">
                    csrf_field()
                    <label for="username">"Username"</label>
                    <input
                        id="username"
                        name="username"
                        type="text"
                        autocomplete="username"
                        required=(true)
                        maxlength="64"
                    />
                    <label for="password">"Password"</label>
                    <input
                        id="password"
                        name="password"
                        type="password"
                        autocomplete="current-password"
                        required=(true)
                        minlength="8"
                    />
                    <button class="button button-primary button-wide" type="submit">
                        (if action == "/setup" {
                            "Create administrator"
                        } else {
                            "Sign in"
                        })
                    </button>
                </form>
            </section>
        }),
        false,
    ))
}
#[page("/setup")]
pub(super) async fn setup_get(cx: &Cx) -> topcoat::Result<impl View> {
    if !db(cx, |a| a.needs_setup()).await.map_err(server_error)? {
        return Err(redirect(href!(crate::web::login::login_get).resolve(cx)).into());
    }
    login_form(
        cx,
        "First setup",
        "Set up your shelf",
        "Create the first administrator account to begin.",
        "/setup",
    )
}
#[page(POST "/setup")]
pub(super) async fn setup_post(cx: &Cx, Form(form): Form<Credentials>) -> topcoat::Result<()> {
    require_csrf(cx, &form.csrf)?;
    if !db(cx, |a| a.needs_setup()).await.map_err(server_error)? {
        return Err(see_other(href!(crate::web::login::login_get).resolve(cx)).into());
    }
    let username = form.username;
    let password = form.password;
    match accepted(db(cx, move |a| a.setup(&username, &password)).await)? {
        true => Err(see_other(
            href!(crate::web::login::login_get)
                .query([("created", "1")])
                .resolve(cx),
        )
        .into()),
        false => Err(see_other(
            href!(crate::web::login::setup_get)
                .query([("error", "invalid")])
                .resolve(cx),
        )
        .into()),
    }
}
#[page("/login")]
pub(super) async fn login_get(cx: &Cx) -> topcoat::Result<impl View> {
    if db(cx, |a| a.needs_setup()).await.map_err(server_error)? {
        return Err(redirect(href!(crate::web::login::setup_get).resolve(cx)).into());
    }
    login_form(cx, "Sign in", "Welcome back", "", "/login")
}
#[page(POST "/login")]
pub(super) async fn login_post(cx: &Cx, Form(form): Form<Credentials>) -> topcoat::Result<()> {
    require_csrf(cx, &form.csrf)?;
    let mut attempt = match login_attempt(cx) {
        Ok(attempt) => attempt,
        Err(_) => {
            return Err(see_other(
                href!(crate::web::login::login_get)
                    .query([("error", "throttled")])
                    .resolve(cx),
            )
            .into());
        }
    };
    let username = form.username;
    let password = form.password;
    match db(cx, move |a| a.login(&username, &password)).await {
        Ok(login) => {
            attempt.finish(true);
            set_session(cx, &login.token);
            Err(see_other(href!(crate::web::recipes::home).resolve(cx)).into())
        }
        Err(error) if error.is::<crate::domain_error::DomainError>() => {
            attempt.finish(false);
            Err(see_other(
                href!(crate::web::login::login_get)
                    .query([("error", "credentials")])
                    .resolve(cx),
            )
            .into())
        }
        Err(error) => Err(server_error(error)),
    }
}
#[page(POST "/logout")]
pub(super) async fn logout(cx: &Cx, Form(form): Form<CsrfOnly>) -> topcoat::Result<()> {
    require_csrf(cx, &form.csrf)?;
    if let Some(token) = session_token(cx) {
        db(cx, move |a| a.logout(&token))
            .await
            .map_err(server_error)?;
    }
    clear_session(cx);
    Err(see_other(href!(crate::web::login::login_get).resolve(cx)).into())
}

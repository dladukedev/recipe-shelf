use super::*;
use topcoat::view::component;

#[derive(Clone, Copy)]
pub(in crate::web) enum SettingsSection {
    Basic,
    Folders,
    Users,
    Templates,
}
impl SettingsSection {
    fn key(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Folders => "folders",
            Self::Users => "users",
            Self::Templates => "templates",
        }
    }
}

fn settings_links(cx: &Cx) -> [(&'static str, &'static str, String); 4] {
    use topcoat::router::href;
    [
        (
            "basic",
            "Basic settings",
            href!(super::basic::settings).resolve(cx),
        ),
        (
            "folders",
            "Folders",
            href!(super::folders::folders_settings).resolve(cx),
        ),
        (
            "templates",
            "Templates",
            href!(super::templates::templates_settings).resolve(cx),
        ),
        (
            "users",
            "User management",
            href!(super::users::users_settings).resolve(cx),
        ),
    ]
}

#[component]
pub(in crate::web) async fn settings_frame(
    cx: &Cx,
    section: SettingsSection,
    user: &User,
    child: Child<'_>,
) -> topcoat::Result<impl View> {
    let feedback = &topcoat::router::query_params::<FeedbackQuery>(cx)?.fields.0;
    let error = &feedback.error;
    let notice = match error.as_deref() {
        Some("folder") => Some(
            "The folder could not be changed. Check its path and make sure it does not overlap another folder.",
        ),
        _ if feedback.synced.is_some() => Some("Sync finished. Review the folder results below."),
        _ if feedback.saved.is_some() => Some("Changes saved."),
        _ if feedback.templates.as_deref() == Some("saved") => Some("Export template saved."),
        _ => None,
    };
    Ok(shell(
        cx,
        "Settings".into(),
        Child::new(view! {
            cx =>
            <h1 class="sr-only">"Settings"</h1>
            if user.is_admin {
                <nav
                    class="settings-tabs"
                    aria-label="Settings sections"
                    data-owner=(user.id.to_string())
                >
                    for (key, label, href) in settings_links(cx) {
                        <a
                            href=(href)
                            class=(if key == section.key() { "active" } else { "" })
                            aria-current=(if key == section.key() {
                                "page"
                            } else {
                                "false"
                            })
                        >
                            (label)
                        </a>
                    }
                </nav>
            }
            if let Some(message) = notice {
                if error.is_some() {
                    <p class="notice notice-error" role="alert">(message)</p>
                } else {
                    snackbar(message: message)
                }
            }
            (child)
        }),
        true,
    ))
}

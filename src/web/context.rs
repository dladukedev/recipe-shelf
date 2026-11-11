use super::*;

pub(super) fn app(cx: &Cx) -> App {
    app_context::<App>(cx).clone()
}
pub(super) async fn db<T: Send + 'static>(
    cx: &Cx,
    work: impl FnOnce(App) -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    let app = app(cx);
    tokio::task::spawn_blocking(move || work(app))
        .await
        .map_err(anyhow::Error::from)?
}

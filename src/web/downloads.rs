use super::*;

pub(super) fn export_error_response(error: anyhow::Error) -> Response {
    let (status, message, retry_after) = match error.downcast_ref::<ExportError>() {
        Some(ExportError::RecipeNotFound) => {
            (http::StatusCode::NOT_FOUND, "Recipe not found.", false)
        }
        Some(ExportError::InvalidFormat) => (
            http::StatusCode::BAD_REQUEST,
            "Choose text, markdown, or pdf.",
            false,
        ),
        Some(ExportError::PdfUnavailable) => (
            http::StatusCode::SERVICE_UNAVAILABLE,
            "PDF generation is unavailable. Ask an administrator to install WeasyPrint or rebuild the supplied container.",
            true,
        ),
        None => (
            http::StatusCode::INTERNAL_SERVER_ERROR,
            "Export could not be generated.",
            false,
        ),
    };
    let mut response = Response::new(Body::from(message));
    *response.status_mut() = status;
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    if retry_after {
        response.headers_mut().insert(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("60"),
        );
    }
    response
}
pub(super) fn export_response(file: ExportFile, inline: bool) -> topcoat::Result<Response> {
    let mut response = Response::new(Body::from(file.bytes));
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static(file.content_type),
    );
    let mode = if inline { "inline" } else { "attachment" };
    let filename: String = file
        .filename
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        .collect();
    let value = http::HeaderValue::from_str(&format!("{mode}; filename=\"{filename}\""))
        .map_err(internal_server_error)?;
    response
        .headers_mut()
        .insert(http::header::CONTENT_DISPOSITION, value);
    Ok(response)
}
#[route(GET "/recipes/{recipe_id}/export/{export_format}")]
pub(super) async fn export_one(cx: &Cx) -> topcoat::Result<Response> {
    let _user = web_user(cx).await?;
    let id = *path_param::<RecipeId>(cx)?;
    let format = path_param::<ExportFormat>(cx);
    if crate::exports::Format::parse(format).is_err() {
        return Err(bad_request("unsupported export format").into());
    }
    let format = format.to_owned();
    match db(cx, move |a| crate::exports::export_one(&a, id, &format)).await {
        Ok(file) => export_response(file, false),
        Err(error) => Ok(export_error_response(error)),
    }
}
#[route(GET "/recipes/{recipe_id}/print")]
pub(super) async fn print_recipe(cx: &Cx) -> topcoat::Result<Response> {
    let _user = web_user(cx).await?;
    let id = *path_param::<RecipeId>(cx)?;
    match db(cx, move |a| crate::exports::export_one(&a, id, "pdf")).await {
        Ok(file) => export_response(file, true),
        Err(error) => Ok(export_error_response(error)),
    }
}
#[route(GET "/settings/export/{export_format}")]
pub(super) async fn export_all(cx: &Cx) -> topcoat::Result<Response> {
    let _user = web_user(cx).await?;
    let format = path_param::<ExportFormat>(cx);
    if crate::exports::Format::parse(format).is_err() {
        return Err(bad_request("unsupported export format").into());
    }
    let format = format.to_owned();
    match db(cx, move |a| crate::exports::export_all(&a, &format)).await {
        Ok(file) => export_response(file, false),
        Err(error) => Ok(export_error_response(error)),
    }
}

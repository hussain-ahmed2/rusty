use rusty::prelude::*;

pub fn middleware(context: &mut RequestContext) -> Result<Option<Response>, ServerError> {
    if context.request().headers().get("x-rusty-block") == Some("1") {
        return Ok(Some(Response::text(
            StatusCode::FORBIDDEN,
            "blocked by middleware",
        )));
    }
    Ok(None)
}

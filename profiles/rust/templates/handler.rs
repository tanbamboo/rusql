use axum::{extract::Path, Json};
use serde::Serialize;

use crate::error::AppError;
use crate::services::{{snake_resource}}::{{Resource}}Service;

#[derive(Serialize)]
pub struct {{Resource}}Response {
    pub id: String,
    pub name: String,
}

/// List all {{resource}} items.
pub async fn list(
    axum::extract::State(svc): axum::extract::State<{{Resource}}Service>,
) -> Result<Json<Vec<{{Resource}}Response>>, AppError> {
    let items = svc.list().await?;
    Ok(Json(items))
}

/// Get a single {{resource}} by id.
pub async fn get(
    axum::extract::State(svc): axum::extract::State<{{Resource}}Service>,
    Path(id): Path<String>,
) -> Result<Json<{{Resource}}Response>, AppError> {
    let item = svc.get(&id).await?;
    Ok(Json(item))
}

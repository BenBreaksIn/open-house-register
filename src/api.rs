use crate::{ApiError, AppState, auth, db, models::*};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use qrcode::{QrCode, render::svg};
use serde_json::json;
use uuid::Uuid;

pub async fn dashboard(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let houses =
        sqlx::query_as::<_, OpenHouse>("SELECT * FROM open_houses ORDER BY starts_at DESC")
            .fetch_all(&state.pool)
            .await?;
    let visitors = sqlx::query_as::<_, Visitor>(
        "SELECT * FROM visitors ORDER BY checked_in_at DESC LIMIT 10000",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        json!({"settings":db::settings(&state.pool).await?,"houses":houses,"visitors":visitors,"base_url":state.config.base_url,"consent_text":CONSENT_TEXT}),
    ))
}
pub async fn save_settings(
    State(state): State<AppState>,
    Json(input): Json<Settings>,
) -> Result<Json<Settings>, ApiError> {
    let settings = input.validate().map_err(ApiError::bad_request)?;
    sqlx::query("UPDATE settings SET data=$1 WHERE id=TRUE")
        .bind(sqlx::types::Json(&settings))
        .execute(&state.pool)
        .await?;
    Ok(Json(settings))
}
pub async fn create_house(
    State(state): State<AppState>,
    Json(input): Json<HouseInput>,
) -> Result<(StatusCode, Json<OpenHouse>), ApiError> {
    let h = input.validate().map_err(ApiError::bad_request)?;
    let house=sqlx::query_as::<_,OpenHouse>("INSERT INTO open_houses(id,address,location,starts_at,ends_at,timezone,photo_url,note,status) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING *")
        .bind(Uuid::new_v4()).bind(h.address).bind(h.location).bind(h.starts_at).bind(h.ends_at).bind(h.timezone).bind(h.photo_url).bind(h.note).bind(h.status).fetch_one(&state.pool).await?;
    Ok((StatusCode::CREATED, Json(house)))
}
pub async fn update_house(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<HouseInput>,
) -> Result<Json<OpenHouse>, ApiError> {
    let h = input.validate().map_err(ApiError::bad_request)?;
    let house=sqlx::query_as::<_,OpenHouse>("UPDATE open_houses SET address=$2,location=$3,starts_at=$4,ends_at=$5,timezone=$6,photo_url=$7,note=$8,status=$9 WHERE id=$1 RETURNING *")
        .bind(id).bind(h.address).bind(h.location).bind(h.starts_at).bind(h.ends_at).bind(h.timezone).bind(h.photo_url).bind(h.note).bind(h.status).fetch_optional(&state.pool).await?.ok_or_else(ApiError::not_found)?;
    Ok(Json(house))
}
async fn public_house(state: &AppState, id: Uuid) -> Result<OpenHouse, ApiError> {
    sqlx::query_as::<_, OpenHouse>("SELECT * FROM open_houses WHERE id=$1 AND status<>'draft'")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::not_found)
}
pub async fn guest_info(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        json!({"house":public_house(&state,id).await?,"settings":db::settings(&state.pool).await?,"consent_text":CONSENT_TEXT}),
    ))
}
pub async fn register(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<Registration>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !auth::is_app_request(&headers) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "Please use the check-in page.",
        ));
    }
    let house = public_house(&state, id).await?;
    if house.status != "open" {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "This open house is no longer accepting check-ins.",
        ));
    }
    if !db::rate_limit(&state.pool, &format!("house:{id}"), 120).await? {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Please wait a moment and try again.",
        ));
    }
    if !input.website.is_empty() {
        return Ok(Json(json!({"ok":true})));
    }
    let input = input
        .validate(&db::settings(&state.pool).await?)
        .map_err(ApiError::bad_request)?;
    // Serialize registration against closing or returning the event to draft.
    let mut tx = state.pool.begin().await?;
    let status: Option<(String,)> =
        sqlx::query_as("SELECT status FROM open_houses WHERE id=$1 FOR SHARE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    match status.as_ref().map(|s| s.0.as_str()) {
        Some("open") => {}
        Some("closed") => {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "This open house is no longer accepting check-ins.",
            ));
        }
        _ => return Err(ApiError::not_found()),
    }
    // Do not overwrite another visitor's saved choices when an email is submitted twice.
    sqlx::query("INSERT INTO visitors(id,house_id,name,email,phone,timeline,represented,follow_up,consent_text) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(house_id,email) DO NOTHING")
        .bind(Uuid::new_v4()).bind(id).bind(input.name).bind(input.email).bind(input.phone).bind(input.timeline).bind(input.represented).bind(input.follow_up).bind(CONSENT_TEXT).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
pub async fn qr(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<Response, ApiError> {
    public_house(&state, id).await?;
    let url = format!("{}/visit/{id}", state.config.base_url);
    let code =
        QrCode::new(url).map_err(|_| ApiError::bad_request("Unable to generate this QR code."))?;
    let svg = code
        .render::<svg::Color>()
        .min_dimensions(400, 400)
        .dark_color(svg::Color("#173b2c"))
        .light_color(svg::Color("#ffffff"))
        .build();
    Ok(([(header::CONTENT_TYPE, "image/svg+xml")], svg).into_response())
}
pub fn csv_cell(value: &str) -> String {
    // Neutralize spreadsheet formulas, including values with leading whitespace.
    if value.trim_start().starts_with(['=', '+', '-', '@']) || value.starts_with(['\t', '\r', '\n'])
    {
        format!("'{value}")
    } else {
        value.to_owned()
    }
}
pub async fn export(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let visitors = sqlx::query_as::<_, Visitor>(
        "SELECT * FROM visitors WHERE house_id=$1 ORDER BY checked_in_at DESC",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record([
        "Name",
        "Email",
        "Phone",
        "Buying timeline",
        "Working with an agent",
        "Follow-up requested",
        "Permission wording shown",
        "Checked in (UTC)",
    ])
    .map_err(ApiError::internal)?;
    for v in visitors {
        w.write_record([
            csv_cell(&v.name),
            csv_cell(&v.email),
            csv_cell(&v.phone),
            csv_cell(&v.timeline),
            csv_cell(&v.represented),
            v.follow_up.to_string(),
            v.consent_text,
            v.checked_in_at.to_rfc3339(),
        ])
        .map_err(ApiError::internal)?;
    }
    let bytes = w.into_inner().map_err(ApiError::internal)?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=visitors.csv",
            ),
        ],
        bytes,
    )
        .into_response())
}
pub async fn delete_visitor(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let result = sqlx::query("DELETE FROM visitors WHERE id=$1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}
pub async fn health(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    sqlx::query("SELECT 1").execute(&state.pool).await?;
    Ok(Json(json!({"status":"ok"})))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_prevents_formulas() {
        for v in ["=1+1", "  @SUM(A1)", "+1", "-1", "\ttest"] {
            assert!(csv_cell(v).starts_with('\''));
        }
        assert_eq!(csv_cell("Jamie Parker"), "Jamie Parker");
    }
}

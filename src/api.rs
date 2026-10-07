use crate::client_ip::ClientKey;
use crate::{ApiError, AppState, auth, db, models::*};
use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
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
    let counts: Vec<(Uuid, i64)> =
        sqlx::query_as("SELECT house_id,COUNT(*) FROM visitors GROUP BY house_id")
            .fetch_all(&state.pool)
            .await?;
    let total: i64 = counts.iter().map(|(_, n)| n).sum();
    let event_counts: std::collections::HashMap<_, _> = counts.into_iter().collect();
    Ok(Json(
        json!({"settings":db::settings(&state.pool).await?,"houses":houses,"visitors":visitors,"base_url":state.config.base_url,"consent_text":CONSENT_TEXT,"capacity":{"event_limit":state.config.event_capacity,"total_limit":state.config.total_capacity,"total":total,"events":event_counts}}),
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
    ClientKey(client): ClientKey,
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
    if !db::rate_limit(&state.pool, &format!("checkin:{client}"), 30).await? {
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
    if !db::rate_limit(&state.pool, "checkin:global", 3000).await? {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Check-in is busy. Please try again shortly.",
        ));
    }
    // Serialize registration against closing or returning the event to draft.
    let mut tx = state.pool.begin().await?;
    // One short, cross-instance lock makes quota checks and insertion atomic.
    // Deletes can only reduce occupancy; they do not need this lock.
    sqlx::query("SELECT pg_advisory_xact_lock(72684301)")
        .execute(&mut *tx)
        .await?;
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
    let (duplicate,): (bool,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM visitors WHERE house_id=$1 AND email=$2)")
            .bind(id)
            .bind(&input.email)
            .fetch_one(&mut *tx)
            .await?;
    if duplicate {
        return Ok(Json(json!({"ok":true})));
    }
    let (event_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM visitors WHERE house_id=$1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let (total_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM visitors")
        .fetch_one(&mut *tx)
        .await?;
    if event_count >= state.config.event_capacity || total_count >= state.config.total_capacity {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "This check-in has reached its capacity. Please ask the host for help.",
        ));
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
    let first = export_batch(&state.pool, id, None).await?;
    let stream = futures_util::stream::try_unfold(
        (state.pool, id, None, Some(first)),
        |(pool, id, after, first)| async move {
            let is_first = first.is_some();
            let rows = match first {
                Some(rows) => rows,
                None => export_batch(&pool, id, after)
                    .await
                    .map_err(|_| std::io::Error::other("CSV export interrupted"))?,
            };
            if rows.is_empty() && !is_first {
                return Ok::<_, std::io::Error>(None);
            }
            let after = rows.last().map(|v| (v.checked_in_at, v.id)).or(after);
            let bytes = csv_batch(&rows, is_first)
                .map_err(|_| std::io::Error::other("CSV export interrupted"))?;
            Ok(Some((bytes, (pool, id, after, None))))
        },
    );
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=visitors.csv",
            ),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

const EXPORT_BATCH_SIZE: i64 = 200;
async fn export_batch(
    pool: &sqlx::PgPool,
    id: Uuid,
    after: Option<(DateTime<Utc>, Uuid)>,
) -> Result<Vec<Visitor>, sqlx::Error> {
    match after {
        None => sqlx::query_as::<_,Visitor>("SELECT * FROM visitors WHERE house_id=$1 ORDER BY checked_in_at DESC,id DESC LIMIT $2").bind(id).bind(EXPORT_BATCH_SIZE).fetch_all(pool).await,
        Some((time, last_id)) => sqlx::query_as::<_,Visitor>("SELECT * FROM visitors WHERE house_id=$1 AND (checked_in_at,id)<($2,$3) ORDER BY checked_in_at DESC,id DESC LIMIT $4").bind(id).bind(time).bind(last_id).bind(EXPORT_BATCH_SIZE).fetch_all(pool).await,
    }
}

fn csv_batch(visitors: &[Visitor], include_header: bool) -> Result<Vec<u8>, ApiError> {
    let mut w = csv::Writer::from_writer(Vec::new());
    if include_header {
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
    }
    for v in visitors {
        w.write_record([
            csv_cell(&v.name),
            csv_cell(&v.email),
            csv_cell(&v.phone),
            csv_cell(&v.timeline),
            csv_cell(&v.represented),
            v.follow_up.to_string(),
            v.consent_text.clone(),
            v.checked_in_at.to_rfc3339(),
        ])
        .map_err(ApiError::internal)?;
    }
    w.into_inner().map_err(ApiError::internal)
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

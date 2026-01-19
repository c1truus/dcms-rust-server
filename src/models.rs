use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub session_ttl_hours: i64,
}

/* -------------------------
   API DTOs
--------------------------*/

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub device_name: Option<String>,
    pub remember_me: Option<bool>, // reserved for future
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub data: LoginResponseData,
}

#[derive(Debug, Serialize)]
pub struct LoginResponseData {
    pub access_token: String,
    pub expires_at: DateTime<Utc>,
    pub dcms_user: UserProfile,
    pub clinic: ClinicProfile,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub data: MeResponseData,
}

#[derive(Debug, Serialize)]
pub struct MeResponseData {
    pub dcms_user: UserProfile,
    pub clinic: ClinicProfile,
    pub session: SessionInfo,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct OkResponse {
    pub data: OkData,
}

#[derive(Debug, Serialize)]
pub struct OkData {
    pub ok: bool,
}

#[derive(Debug, Serialize)]
pub struct UserProfile {
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    /// We currently store a single role as smallint in DB.
    /// For backward compatibility with your earlier API, we return an array.
    pub roles: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ClinicProfile {
    pub clinic_name: String,
}

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub session_token_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

/* -------------------------
   DB Row Models
--------------------------*/

#[derive(Debug, sqlx::FromRow)]
pub struct UserRow {
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    pub roles: i16,
    pub is_active: bool,
}

#[derive(Debug, sqlx::FromRow)]
pub struct SessionTokenRow {
    pub session_token_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PhoneNumberRow {
    pub phone_number_id: Uuid,
    pub patient_id: Uuid,
    pub phone_number: String,
    pub label: String,
    pub is_primary: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "smallint")]
#[repr(i16)]
pub enum SmsDirection {
    Receive = 0,
    Send = 1,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SmsRow {
    pub sms_id: Uuid,
    pub phone_number_id: Uuid,
    pub direction: SmsDirection,
    pub sent_at: DateTime<Utc>, // ✅ matches DB column name
    pub subject: Option<String>,
    pub sms_text: String,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>, // ✅ your SQL RETURNING includes created_at
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServiceCatalogRow {
    pub service_id: Uuid,
    pub service_type: String,
    pub display_number: i32,
    pub display_name: String,
    pub default_duration_min: Option<i32>,
    pub disclaimer: Option<String>,
    pub price_cents: i32,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/* -------------------------
   Helpers
--------------------------*/

/// Role mapping according to your DB spec:
/// 0 Patient, 1 Admin, 2 Manager, 3 Doctor, 4 Receptionist
pub fn role_to_string(role: i16) -> String {
    match role {
        0 => "patient",
        1 => "admin",
        2 => "manager",
        3 => "doctor",
        4 => "receptionist",
        _ => "unknown",
    }
    .to_string()
}

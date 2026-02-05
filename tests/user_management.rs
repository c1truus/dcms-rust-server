// // tests/user_management.rs
// use dcms_rust_server::{
//     models::{AppState, UserRow},
//     routes::user_routes::*,
// };
// use sqlx::PgPool;
// use uuid::Uuid;

// // Setup test database (you can use testcontainers or a test DB)
// async fn setup_test_db() -> PgPool {
//     // Implementation depends on your setup
//     // Could use sqlx::test or testcontainers
// }

// #[tokio::test]
// async fn test_list_users_requires_admin() {
//     // Test that non-admin users can't list users
//     // You'd need to mock AuthContext here
// }
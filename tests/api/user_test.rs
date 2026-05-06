use super::{common, mock};
use mockall::mock;
use wildcard_backend::{api::TestApp, TestUser};

trait UserRepository {
    fn exists(&self, username: &str) -> bool;
    fn save(&self, username: &str) -> bool;
}

mock! {
    Repo {}

    impl UserRepository for Repo {
        fn exists(&self, username: &str) -> bool;
        fn save(&self, username: &str) -> bool;
    }
}

#[test]
fn user_fixture_can_be_loaded_from_json() {
    let user = common::load_fixture::<TestUser>("user_sample.json");

    assert_eq!(user.username, "test_user");
    assert_eq!(user.email, "test@example.com");
}

#[test]
fn user_registration_smoke_test_returns_created() {
    let mut app = TestApp::new();
    let payload = mock::test_data::register_user_payload();

    let response = app.register_user(&payload.username);

    assert_eq!(response.status_code, 201);
}

#[test]
fn user_repository_contract_can_be_mocked() {
    let payload = mock::test_data::register_user_payload();
    let expected_exists = payload.username.clone();
    let expected_save = payload.username.clone();
    let mut repo = MockRepo::new();

    repo.expect_exists()
        .withf(move |username| username == expected_exists)
        .return_const(false);

    repo.expect_save()
        .withf(move |username| username == expected_save)
        .return_const(true);

    assert!(!repo.exists(&payload.username));
    assert!(repo.save(&payload.username));
}
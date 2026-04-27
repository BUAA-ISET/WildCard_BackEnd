use super::mock;
use rstest::rstest;
use wildcard_backend::api::TestApp;

#[rstest]
#[case("room_001")]
#[case("room_alpha")]
fn room_creation_smoke_test_returns_created(#[case] room_id: &str) {
    let mut app = TestApp::new();

    let response = app.create_room(room_id);

    assert_eq!(response.status_code, 201);
}

#[test]
fn room_identifiers_from_mock_data_are_stable() {
    assert_eq!(mock::test_data::room_identifier(), "room_001");
}

#[test]
fn duplicate_room_returns_conflict() {
    let mut app = TestApp::new();
    let room_id = mock::test_data::room_identifier();

    let first = app.create_room(room_id);
    let second = app.create_room(room_id);

    assert_eq!(first.status_code, 201);
    assert_eq!(second.status_code, 409);
}

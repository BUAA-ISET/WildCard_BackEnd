trait SessionStore {
    fn save_session(&mut self, user_id: u64) -> bool;
}

#[derive(Default)]
struct FakeSessionStore {
    saved_user_ids: Vec<u64>,
}

impl SessionStore for FakeSessionStore {
    fn save_session(&mut self, user_id: u64) -> bool {
        self.saved_user_ids.push(user_id);
        true
    }
}

#[test]
fn fake_store_can_record_backend_side_effects() {
    let mut store = FakeSessionStore::default();

    assert!(store.save_session(1));
    assert_eq!(store.saved_user_ids, vec![1]);
}

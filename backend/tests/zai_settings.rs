use backend::infra::zai_settings::SettingsState;

#[test]
fn empty_values_do_not_overwrite() {
    let state = SettingsState::load();
    let before = state.zai();
    state.set_zai(Some(String::new()), Some(String::new())).ok();
    assert_eq!(state.zai(), before);
}

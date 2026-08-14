use super::*;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;

#[test]
fn pet_load_without_runtime_sends_completion_event() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let app_event_tx = AppEventSender::new(tx);

    spawn_pet_load(
        async { Ok::<Option<crate::pets::AmbientPet>, String>(None) },
        app_event_tx,
        |result| AppEvent::ConfiguredPetLoaded {
            pet_id: crate::pets::DEFAULT_PET_ID.to_string(),
            result,
        },
    );

    match rx.blocking_recv().expect("pet load completion event") {
        AppEvent::ConfiguredPetLoaded { pet_id, result } => {
            assert_eq!(pet_id, crate::pets::DEFAULT_PET_ID);
            assert!(result.expect("successful pet load").is_none());
        }
        event => panic!("expected configured pet completion, got {event:?}"),
    }
}

#[tokio::test]
async fn shared_pet_load_uses_cached_builtin_assets() {
    let (chat, _tx, _rx, _op_rx) =
        crate::chatwidget::tests::make_chatwidget_manual_with_sender().await;
    let codex_home = tempfile::tempdir().unwrap();
    crate::pets::write_test_pack(codex_home.path());

    let pet_http_client = codex_http_client::RouteAwareClientPool::new(
        codex_http_client::HttpClientFactory::new(
            codex_http_client::OutboundProxyPolicy::ReqwestDefault,
        ),
        codex_http_client::ClientRouteClass::Other,
    );
    crate::pets::load_pet_with_assets(
        crate::pets::DEFAULT_PET_ID.to_string(),
        AbsolutePathBuf::from_absolute_path(codex_home.path()).expect("absolute temporary path"),
        chat.frame_requester.clone(),
        /*animations_enabled*/ false,
        &pet_http_client,
    )
    .await
    .expect("load cached built-in pet");

    assert!(
        codex_home
            .path()
            .join("cache")
            .join("tui-pets")
            .join("frame-cache")
            .join(crate::pets::DEFAULT_PET_ID)
            .is_dir()
    );
}

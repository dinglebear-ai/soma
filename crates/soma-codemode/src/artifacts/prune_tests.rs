use super::prune::prune_old_runs;

#[tokio::test]
async fn prune_noops_when_root_missing() {
    let root = tempfile::tempdir().unwrap().path().join("missing");
    assert_eq!(prune_old_runs(&root, 2).await.unwrap(), 0);
}

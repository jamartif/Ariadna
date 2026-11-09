#![allow(clippy::unwrap_used)]
mod common;

#[test]
fn el_helper_crea_un_repo_con_commit() {
    let r = common::TestRepo::with_initial_commit();
    assert_eq!(r.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(), "main");
    assert!(r.path().join("README.md").exists());
}

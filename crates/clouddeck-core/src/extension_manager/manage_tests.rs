use std::fs;

use clouddeck_extension_api::API_VERSION;

use crate::extension_host::{StateStore, state_path};
use crate::release::sign::generate_keypair;

use super::support::*;

#[test]
fn successful_update_keeps_the_enabled_value() {
    let fx = fixture();
    fx.install_v1();
    fx.manager().enable_with("toy", true).unwrap();
    fx.publish_release(
        &fx.author.0,
        "0.2.0",
        &standard_archive("toy", API_VERSION, "0.2.0"),
    );
    fx.manager().update("toy", None).unwrap();
    assert!(fx.state_enabled());
    assert!(fx.root.join("toy/extension.toml").is_file());
}

#[test]
fn enable_is_refused_while_external_extensions_are_off() {
    let fx = fixture();
    fx.install_v1();
    let manager = fx.manager();
    let err = manager.enable("toy").unwrap_err();
    assert!(err.contains("security review B"), "{err}");
    assert!(!fx.state_enabled());

    manager.enable_with("toy", true).unwrap();
    assert!(fx.state_enabled());
}

#[test]
fn dev_extension_is_gated_the_same_way() {
    let fx = fixture();
    let source = tempfile::tempdir().unwrap();
    fs::write(
        source.path().join("extension.toml"),
        extension_toml("toy", API_VERSION, "0.1.0"),
    )
    .unwrap();
    fs::write(source.path().join(library_name("toy")), b"fake").unwrap();
    fx.manager().install_dev(source.path()).unwrap();
    assert!(fx.manager().enable("toy").is_err());
    assert!(!fx.state_enabled());
}

#[test]
fn disable_remove_and_list_keep_the_pin() {
    let fx = fixture();
    fx.install_v1();
    let manager = fx.manager();
    manager.enable_with("toy", true).unwrap();

    let (listed, refusals) = manager.list().unwrap();
    assert_eq!(refusals.len(), 1);
    assert!(
        refusals[0]
            .message
            .contains("external extensions are turned off")
    );
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "toy");
    assert!(listed[0].enabled && !listed[0].dev);
    assert_eq!(listed[0].author_key.as_deref(), Some(fx.author.1.as_str()));

    manager.disable("toy").unwrap();
    assert!(!fx.state_enabled());

    manager.remove("toy").unwrap();
    assert!(!fx.root.join("toy").exists());
    assert_eq!(fx.installed_pin().as_deref(), Some(fx.author.1.as_str()));
    assert!(manager.list().unwrap().0.is_empty());
}

#[test]
fn reinstall_after_remove_cannot_swap_the_pinned_key() {
    let fx = fixture();
    fx.install_v1();
    fx.manager().remove("toy").unwrap();

    let new_key = generate_keypair();
    fx.publish_index(&new_key.1);
    fx.publish_release(
        &new_key.0,
        "0.1.0",
        &standard_archive("toy", API_VERSION, "0.1.0"),
    );
    assert!(fx.manager().install_from_index("toy", None).is_err());
    assert!(!fx.root.join("toy").exists());
}

#[test]
fn unknown_or_hostile_ids_are_refused_before_touching_disk() {
    let fx = fixture();
    fx.install_v1();
    let manager = fx.manager();
    assert!(manager.remove("../toy").is_err());
    assert!(manager.disable("nope").is_err());
    assert!(fx.root.join("toy").exists());
}

#[test]
fn dev_install_copies_and_refuses_symlinks() {
    let fx = fixture();
    let source = tempfile::tempdir().unwrap();
    fs::write(
        source.path().join("extension.toml"),
        extension_toml("toy", API_VERSION, "0.1.0"),
    )
    .unwrap();
    fs::write(source.path().join(library_name("toy")), b"fake").unwrap();
    fx.manager().install_dev(source.path()).unwrap();

    let installed = fs::symlink_metadata(fx.root.join("toy")).unwrap();
    assert!(installed.is_dir() && !installed.file_type().is_symlink());
    let dev = StateStore::open(&state_path(&fx.state_root, "toy"))
        .unwrap()
        .read(|s| s.dev);
    assert!(dev);

    #[cfg(unix)]
    {
        let linked = tempfile::tempdir().unwrap();
        fs::write(
            linked.path().join("extension.toml"),
            extension_toml("other", API_VERSION, "0.1.0"),
        )
        .unwrap();
        fs::write(linked.path().join(library_name("other")), b"fake").unwrap();
        std::os::unix::fs::symlink(source.path(), linked.path().join("link")).unwrap();
        assert!(
            fx.manager()
                .install_dev(linked.path())
                .unwrap_err()
                .contains("symbolic link")
        );
        assert!(!fx.root.join("other").exists());
    }
}

fn hold_install_lock(fx: &Fixture) -> fs::File {
    fs::create_dir_all(&fx.state_root).unwrap();
    let held = fs::File::create(fx.state_root.join(".install.lock")).unwrap();
    held.try_lock().unwrap();
    held
}

#[test]
fn remove_is_refused_while_another_operation_holds_the_lock() {
    let fx = fixture();
    fx.install_v1();
    let held = hold_install_lock(&fx);
    let err = fx.manager().remove("toy").unwrap_err();
    assert!(
        err.contains("another extension operation is running"),
        "{err}"
    );
    assert!(fx.root.join("toy").exists());

    drop(held);
    fx.manager().remove("toy").unwrap();
    assert!(!fx.root.join("toy").exists());
}

#[test]
fn dev_install_is_refused_while_another_operation_holds_the_lock() {
    let fx = fixture();
    let source = tempfile::tempdir().unwrap();
    fs::write(
        source.path().join("extension.toml"),
        extension_toml("toy", API_VERSION, "0.1.0"),
    )
    .unwrap();
    fs::write(source.path().join(library_name("toy")), b"fake").unwrap();
    let held = hold_install_lock(&fx);
    let err = fx.manager().install_dev(source.path()).unwrap_err();
    assert!(
        err.contains("another extension operation is running"),
        "{err}"
    );
    assert!(!fx.root.join("toy").exists());
    drop(held);
}

#[test]
fn dev_install_refuses_to_replace_a_signed_install() {
    let fx = fixture();
    fx.install_v1();
    let source = tempfile::tempdir().unwrap();
    fs::write(
        source.path().join("extension.toml"),
        extension_toml("toy", API_VERSION, "0.1.0"),
    )
    .unwrap();
    fs::write(source.path().join(library_name("toy")), b"fake").unwrap();
    assert!(fx.manager().install_dev(source.path()).is_err());
}

use crate::release::sign::generate_keypair;
use crate::release::target::current_target as target_now;

use clouddeck_extension_api::API_VERSION;

use crate::extension_host::{StateStore, state_path};
use crate::extension_manager::install::{EXTERNAL_EXTENSIONS_ALLOWED, Outcome};

use super::support::*;

const _: () = assert!(!EXTERNAL_EXTENSIONS_ALLOWED);

#[test]
fn first_install_by_index_id_pins_the_key_and_stays_disabled() {
    let fx = fixture();
    fx.install_v1();
    assert!(fx.root.join("toy/extension.toml").is_file());
    assert!(fx.root.join("toy").join(library_name("toy")).is_file());
    assert_eq!(fx.installed_pin().as_deref(), Some(fx.author.1.as_str()));
    assert!(!fx.state_enabled());
}

#[test]
fn manifest_signed_by_another_key_is_refused_and_nothing_is_created() {
    let fx = fixture();
    fx.publish_index(&fx.author.1);
    let other = generate_keypair();
    fx.publish_release(
        &other.0,
        "0.1.0",
        &standard_archive("toy", API_VERSION, "0.1.0"),
    );
    assert!(fx.manager().install_from_index("toy", None).is_err());
    assert!(!fx.root.join("toy").exists());
}

#[test]
fn sha256_and_size_mismatches_leave_the_installed_folder_untouched() {
    let fx = fixture();
    fx.install_v1();
    let before = read(&fx.root.join("toy/extension.toml"));

    let bundle = standard_archive("toy", API_VERSION, "0.2.0");
    fx.publish_release(&fx.author.0, "0.2.0", &bundle);
    {
        let mut hub = fx.hub.lock().unwrap();
        let name = format!("toy-0.2.0-{}.tar.gz", target_now().unwrap().id());
        let asset = hub.assets.get_mut(&name).unwrap();
        asset[0] ^= 0xff;
    }
    assert!(
        fx.manager()
            .install_from_index("toy", None)
            .unwrap_err()
            .contains("checksum")
    );
    assert_eq!(read(&fx.root.join("toy/extension.toml")), before);
    assert_eq!(fx.staging_leftovers(), 0);

    fx.publish_release(&fx.author.0, "0.2.0", &bundle);
    {
        let mut hub = fx.hub.lock().unwrap();
        let name = format!("toy-0.2.0-{}.tar.gz", target_now().unwrap().id());
        hub.assets.insert(name, bundle[..bundle.len() - 1].to_vec());
    }
    assert!(
        fx.manager()
            .install_from_index("toy", None)
            .unwrap_err()
            .contains("size")
    );
    assert_eq!(read(&fx.root.join("toy/extension.toml")), before);
    assert_eq!(fx.staging_leftovers(), 0);
}

#[test]
fn url_outside_the_index_needs_trust_key_and_mismatched_key_is_refused() {
    let fx = fixture();
    fx.publish_index(&fx.author.1);
    fx.publish_release(
        &fx.author.0,
        "0.1.0",
        &standard_archive("toy", API_VERSION, "0.1.0"),
    );
    let manager = fx.manager();

    let unlisted = "https://github.com/someone/else";
    assert!(
        manager
            .install_from_url(unlisted, None, None)
            .unwrap_err()
            .contains("--trust-key")
    );

    let err = manager
        .install_from_url(unlisted, Some(&fx.author.1), None)
        .unwrap_err();
    assert!(err.contains("the index lists toy at"), "{err}");
    assert_eq!(fx.installed_pin(), None);

    let other = generate_keypair().1;
    let err = manager
        .install_from_url(REPO, Some(&other), None)
        .unwrap_err();
    assert!(err.contains("does not match the index key"), "{err}");
}

#[test]
fn trust_key_installs_unlisted_url_and_pins_it() {
    let fx = fixture();
    fx.publish_empty_index();
    fx.publish_release(
        &fx.author.0,
        "0.1.0",
        &standard_archive("toy", API_VERSION, "0.1.0"),
    );

    let outcome = fx
        .manager()
        .install_from_url("https://github.com/someone/else", Some(&fx.author.1), None)
        .unwrap();

    assert!(matches!(outcome, Outcome::Installed(_)));
    assert!(fx.root.join("toy/extension.toml").is_file());
    assert_eq!(fx.installed_pin().as_deref(), Some(fx.author.1.as_str()));
    assert!(!fx.state_enabled());
}

#[test]
fn update_signed_by_a_new_key_needs_the_offered_key_to_be_accepted() {
    let fx = fixture();
    fx.install_v1();
    let new_key = generate_keypair();
    fx.publish_index(&new_key.1);
    fx.publish_release(
        &new_key.0,
        "0.2.0",
        &standard_archive("toy", API_VERSION, "0.2.0"),
    );
    let manager = fx.manager();
    manager.enable_with("toy", true).unwrap();

    let err = manager.update("toy", None).unwrap_err();
    assert!(
        err.contains(&fx.author.1) && err.contains(&new_key.1),
        "{err}"
    );
    assert_eq!(fx.installed_pin().as_deref(), Some(fx.author.1.as_str()));

    let outcome = manager.update("toy", Some(&new_key.1)).unwrap();
    assert!(matches!(outcome, Outcome::Installed(_)));
    assert_eq!(fx.installed_pin().as_deref(), Some(new_key.1.as_str()));
    assert!(!fx.state_enabled());
}

#[test]
fn archive_with_another_id_or_api_is_refused_and_the_old_folder_survives() {
    let fx = fixture();
    fx.install_v1();
    let before = read(&fx.root.join("toy/extension.toml"));
    let manager = fx.manager();

    fx.publish_release(
        &fx.author.0,
        "0.2.0",
        &standard_archive("impostor", API_VERSION, "0.2.0"),
    );
    assert!(
        manager
            .update("toy", None)
            .unwrap_err()
            .contains("impostor")
    );
    assert_eq!(read(&fx.root.join("toy/extension.toml")), before);

    fx.publish_release(
        &fx.author.0,
        "0.2.0",
        &standard_archive("toy", API_VERSION + 1, "0.2.0"),
    );
    assert!(
        manager
            .update("toy", None)
            .unwrap_err()
            .contains("Rebuild toy")
    );
    assert_eq!(read(&fx.root.join("toy/extension.toml")), before);
}

#[test]
fn shipped_state_json_grants_nothing() {
    let fx = fixture();
    fx.publish_index(&fx.author.1);
    let state = br#"{"enabled":true,"granted":[{"kind":"credentials"}]}"#;
    let bundle = archive(&[
        (
            "extension.toml",
            extension_toml("toy", API_VERSION, "0.1.0").as_bytes(),
        ),
        (&library_name("toy"), b"fake library"),
        ("state.json", state),
    ]);
    fx.publish_release(&fx.author.0, "0.1.0", &bundle);
    fx.manager().install_from_index("toy", None).unwrap();

    assert!(!fx.root.join("toy/state.json").exists());
    let granted = StateStore::open(&state_path(&fx.state_root, "toy"))
        .unwrap()
        .read(|s| (s.enabled, s.granted.is_empty()));
    assert_eq!(granted, (false, true));
}

#[test]
fn traversal_during_update_leaves_the_previous_install_intact() {
    let fx = fixture();
    fx.install_v1();
    let before = read(&fx.root.join("toy/extension.toml"));
    let bad = traversal_archive(extension_toml("toy", API_VERSION, "0.2.0").as_bytes());
    fx.publish_release(&fx.author.0, "0.2.0", &bad);

    assert!(fx.manager().update("toy", None).is_err());
    assert_eq!(read(&fx.root.join("toy/extension.toml")), before);
    assert_eq!(fx.staging_leftovers(), 0);
    assert!(!fx.root.join("escape").exists());
}

#[test]
fn update_that_is_not_newer_changes_nothing() {
    let fx = fixture();
    fx.install_v1();
    fx.publish_release(
        &fx.author.0,
        "0.1.0",
        &standard_archive("toy", API_VERSION, "0.1.0"),
    );
    let outcome = fx.manager().update("toy", None).unwrap();
    assert_eq!(
        outcome,
        Outcome::UpToDate {
            version: "0.1.0".to_string()
        }
    );
}

use std::path::PathBuf;

use clouddeck_core::load_extension;

fn built_toy_library() -> PathBuf {
    let test_exe = std::env::current_exe().unwrap();
    let target_profile_dir = test_exe.parent().unwrap().parent().unwrap();
    let library = target_profile_dir.join(format!(
        "{}clouddeck_extension_toy.{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_EXTENSION
    ));
    assert!(
        library.exists(),
        "expected the toy cdylib at {}",
        library.display()
    );
    library
}

#[test]
fn host_loads_the_toy_cdylib() {
    let loaded = load_extension(&built_toy_library()).expect("toy extension should load");
    assert!(loaded.path().ends_with(built_toy_library().file_name().unwrap()));
}

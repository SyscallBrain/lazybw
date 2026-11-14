//! Documentation stays in sync with the code.

#[test]
fn configuration_doc_contains_the_default_config() {
    let config = include_str!("../assets/default-config.toml");
    let doc = include_str!("../../../book/src/reference/config.md");
    assert!(
        doc.contains(config),
        "book/src/reference/config.md is out of date: copy assets/default-config.toml into it"
    );
}

#[test]
fn every_builtin_theme_has_a_file() {
    for name in lazybw::theme::builtin::names() {
        if name == "terminal" {
            continue;
        }
        let path = format!("{}/assets/themes/{name}.toml", env!("CARGO_MANIFEST_DIR"));
        assert!(std::path::Path::new(&path).exists(), "missing {path}");
    }
}

#[test]
fn check_script_is_executable() {
    use std::os::unix::fs::PermissionsExt;
    let path = format!("{}/../../scripts/check.sh", env!("CARGO_MANIFEST_DIR"));
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert!(mode & 0o111 != 0, "scripts/check.sh is not executable");
}

#[test]
fn readme_screenshots_exist() {
    for path in [
        format!("{}/assets/screenshots/vault.svg", env!("CARGO_MANIFEST_DIR")),
        format!("{}/assets/screenshots/editor.svg", env!("CARGO_MANIFEST_DIR")),
    ] {
        let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing {path}"));
        assert!(text.contains("<svg"), "{path} is not an SVG");
    }
    let demo = format!("{}/../../book/src/images/demo.gif", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&demo).unwrap_or_else(|_| panic!("missing {demo}"));
    assert!(bytes.starts_with(b"GIF89a"), "{demo} is not a GIF");
    let readme = include_str!("../../../README.md");
    assert!(readme.contains("book/src/images/demo.gif"));
    assert!(readme.contains("assets/screenshots/editor.svg"));
}

#[test]
fn release_artifacts_exist() {
    let version = format!("pkgver={}", env!("CARGO_PKG_VERSION"));
    let source = include_str!("../../../packaging/aur/lazybw/PKGBUILD");
    assert!(source.contains("pkgname=lazybw\n"));
    assert!(source.contains("cargo build --frozen --release"));
    assert!(source.contains(&version), "packaging/aur/lazybw: {version} expected");
    let binary = include_str!("../../../packaging/aur/lazybw-bin/PKGBUILD");
    assert!(binary.contains("pkgname=lazybw-bin"));
    assert!(binary.contains(&version), "packaging/aur/lazybw-bin: {version} expected");
    let checklist = include_str!("../../../docs/release.md");
    assert!(checklist.contains("cargo package"));
}

#[test]
fn the_book_has_the_current_screenshots_and_logo() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let shots = std::fs::read_dir(format!("{}/assets/screenshots", env!("CARGO_MANIFEST_DIR")))
        .unwrap()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".svg"))
        .collect::<Vec<_>>();
    assert!(shots.len() >= 8, "expected the README gallery, found {shots:?}");
    for name in &shots {
        let crate_copy =
            std::fs::read(format!("{}/assets/screenshots/{name}", env!("CARGO_MANIFEST_DIR")))
                .unwrap();
        let book_copy =
            std::fs::read(format!("{root}/book/src/images/screenshots/{name}")).unwrap_or_default();
        assert!(
            crate_copy == book_copy,
            "book/src/images/screenshots/{name} is stale: run `cargo run -p lazybw --example screenshots`"
        );
    }
    for name in ["logo.svg", "logo-mark.svg", "logo-wordmark.svg", "logo-wordmark-light.svg"] {
        let path = format!("{root}/book/src/images/{name}");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing {path}"));
        assert!(text.contains("<svg"), "{path} is not an SVG");
    }
}

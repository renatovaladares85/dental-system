use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    println!("cargo:rerun-if-changed=../dist");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    let generated_assets = output.join("embedded_assets");
    fs::create_dir_all(&generated_assets).expect("create generated asset directory");

    let source = Path::new("../dist");
    let mut assets = Vec::new();
    if source.is_dir() {
        collect_assets(source, source, &generated_assets, &mut assets);
    }

    let release = env::var("PROFILE").as_deref() == Ok("release");
    if release {
        validate_release_assets(&assets);
    }
    if assets.is_empty() {
        let placeholder = generated_assets.join("index.html");
        fs::write(
            &placeholder,
            b"<!doctype html><html lang=\"pt-BR\"><meta charset=\"utf-8\"><title>Offline Dental System</title><body>Execute o build do frontend antes de iniciar o servidor.</body></html>",
        )
        .expect("write fallback page");
        assets.push(("index.html".to_owned(), placeholder));
    }

    assets.sort_by(|left, right| left.0.cmp(&right.0));
    let mut generated = String::from(
        "pub fn embedded_asset(path: &str) -> Option<(&'static [u8], &'static str)> {\n    match path {\n",
    );
    for (path, copied) in assets {
        let mime = content_type(&path);
        generated.push_str(&format!(
            "        {path:?} => Some((include_bytes!({copied:?}), {mime:?})),\n",
            copied = copied.to_string_lossy(),
        ));
    }
    generated.push_str("        _ => None,\n    }\n}\n");
    fs::write(output.join("embedded_assets.rs"), generated).expect("write asset lookup");
}

fn collect_assets(root: &Path, current: &Path, output: &Path, assets: &mut Vec<(String, PathBuf)>) {
    for entry in fs::read_dir(current).expect("read frontend dist") {
        let entry = entry.expect("read frontend entry");
        let path = entry.path();
        if path.is_dir() {
            collect_assets(root, &path, output, assets);
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) == Some("map") {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("asset is inside dist")
            .to_string_lossy()
            .replace('\\', "/");
        println!("cargo:rerun-if-changed={}", path.display());
        let destination = output.join(relative.replace('/', "__"));
        fs::copy(&path, &destination).expect("copy frontend asset");
        assets.push((relative, destination));
    }
}

fn validate_release_assets(assets: &[(String, PathBuf)]) {
    let contains = |required: &str| assets.iter().any(|(path, _)| path == required);
    let versioned_javascript = assets
        .iter()
        .any(|(path, _)| is_versioned_asset(path, &["js", "mjs"]));
    let versioned_css = assets
        .iter()
        .any(|(path, _)| is_versioned_asset(path, &["css"]));
    if !contains("index.html")
        || !contains("manifest.webmanifest")
        || !contains("service-worker.js")
        || !versioned_javascript
        || !versioned_css
    {
        panic!(
            "release builds require index.html, manifest.webmanifest, service-worker.js and versioned JavaScript/CSS assets from a completed frontend build"
        );
    }
}

fn is_versioned_asset(path: &str, extensions: &[&str]) -> bool {
    if !path.starts_with("assets/") || path.contains("..") {
        return false;
    }
    let file = Path::new(path);
    let extension = file.extension().and_then(|value| value.to_str());
    if !extension.is_some_and(|value| extensions.contains(&value)) {
        return false;
    }
    file.file_stem()
        .and_then(|value| value.to_str())
        .and_then(|stem| stem.split_once('-'))
        .is_some_and(|(_, hash)| {
            hash.len() >= 8
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        })
}

fn content_type(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|value| value.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("ico") => "image/x-icon",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("webmanifest") => "application/manifest+json; charset=utf-8",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::is_versioned_asset;

    #[test]
    fn accepts_vite_hashes_that_contain_a_hyphen() {
        assert!(is_versioned_asset("assets/index-BE-ywWTb.css", &["css"]));
        assert!(is_versioned_asset(
            "assets/index-c5SFIbdY.js",
            &["js", "mjs"]
        ));
    }

    #[test]
    fn rejects_non_versioned_or_unsafe_assets() {
        assert!(!is_versioned_asset("assets/index.css", &["css"]));
        assert!(!is_versioned_asset(
            "assets/../index-abcdefgh.css",
            &["css"]
        ));
    }
}

//! Package and smoke-test through RackForge's own Rust tools.
use std::{error::Error, fs, path::Path, process::Command};

pub fn build() -> Result<(), Box<dyn Error>> {
    let root = workspace_root()?;
    build_to(
        &root
            .join("dist")
            .join(format!("RF-Tines-{}.rfplugin", env!("CARGO_PKG_VERSION"))),
    )
}

pub(crate) fn workspace_root() -> Result<std::path::PathBuf, Box<dyn Error>> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("cannot resolve the workspace root")?
        .to_path_buf())
}

pub(crate) fn build_to(output: &Path) -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("cannot resolve the workspace root")?;
    let host = root
        .parent()
        .ok_or("cannot resolve sibling host")?
        .join("rackforge");
    let tools = host.join("target/release");
    let store = tools.join(format!("rackforge-store{}", std::env::consts::EXE_SUFFIX));
    let core = tools.join(format!("rackforge-core{}", std::env::consts::EXE_SUFFIX));
    let component = root.join("target/wasm32-unknown-unknown/release/rf_tines_plugin.wasm");
    let package = root.join("package");
    let dist = root.join("dist");
    if output.exists() {
        return Err(format!("refusing to overwrite {}", output.display()).into());
    }
    super::web_ui::build()?;
    // Build the component here rather than trusting whatever is on disk. It
    // used to be checked for existence only, so a stale artifact from an
    // earlier version packaged silently, and the only tell was the smoke
    // test's peak changing by a ten-thousandth.
    run(Command::new("cargo").current_dir(root).args([
        "build",
        "--locked",
        "--release",
        "--target",
        "wasm32-unknown-unknown",
        "-p",
        "rf-tines-plugin",
    ]))?;
    for file in [&store, &core] {
        if !file.is_file() {
            return Err(format!("build the required artifact first: {}", file.display()).into());
        }
    }
    if !component.is_file() {
        return Err(format!("the component did not build: {}", component.display()).into());
    }
    // Belt and braces. `CARGO_PKG_VERSION` is compiled into the component as
    // the program descriptor's plugin_version, so a component built at
    // another version does not carry this string. This catches a build that
    // reported success without replacing the artifact, which a freshness
    // check on the file's timestamp would not.
    verify_component_version(&component)?;
    fs::create_dir_all(&dist)?;
    fs::copy(&component, package.join("component.wasm"))?;
    run(Command::new(&core).arg("inspect").arg(&package))?;
    run(Command::new(&core)
        .arg("smoke")
        .arg(&package)
        .arg("--preset")
        .arg("portable-bark-1972")
        .arg("--data-root")
        .arg(dist.join("smoke-data")))?;
    run(Command::new(&store)
        .arg("pack-wasm")
        .arg(&package)
        .arg(&component)
        .arg(output))?;
    println!("Validated package: {}", output.display());
    Ok(())
}

/// Fails unless the built component carries this crate's version string.
fn verify_component_version(component: &Path) -> Result<(), Box<dyn Error>> {
    let version = env!("CARGO_PKG_VERSION");
    let bytes = fs::read(component)?;
    if bytes
        .windows(version.len())
        .any(|window| window == version.as_bytes())
    {
        return Ok(());
    }
    Err(format!(
        "{} does not carry version {version}; it was built from other sources",
        component.display()
    )
    .into())
}

pub(crate) fn run(command: &mut Command) -> Result<(), Box<dyn Error>> {
    hide_console(command);
    let status = command.status()?;
    if !status.success() {
        return Err(format!("RackForge validation failed: {status}").into());
    }
    Ok(())
}

pub(crate) fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW for CLI helpers.
    }
    #[cfg(not(windows))]
    let _ = command;
}

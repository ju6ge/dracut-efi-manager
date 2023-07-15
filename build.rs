use std::{env, path::{Path, PathBuf}, fs::File, io::{Write, Error}};

use askama::Template;

#[derive(Debug, Template)]
#[template(path="90-dracut-efibin-install.hook", escape="none")]
#[allow(dead_code)]
struct PacmanInstallHook {
    prefix: String
}

#[derive(Debug, Template)]
#[template(path="90-dracut-efibin-clean.hook", escape="none")]
#[allow(dead_code)]
struct PacmanCleanHook {
    prefix: String
}

fn write_to_file(path: &Path, content: &dyn ToString) -> Result<(), Error> {
    let parent_dir = path.parent().unwrap();
    if !parent_dir.exists() {
        std::fs::create_dir_all(parent_dir)?;
    }

    let mut file = File::create(path)?;
    file.write_all(content.to_string().as_bytes())?;
    Ok(())
}

fn get_current_binary_directory() -> PathBuf {
    let out_dir = std::env::var("OUT_DIR").expect("Failed to retrieve OUT_DIR");
    let path = Path::new(&out_dir);
    path.parent().and_then(|p| p.parent()).and_then(|p| p.parent()).and_then(|p| p.canonicalize().ok()).unwrap()
}

fn main() {
    let prefix = env::var("PREFIX").unwrap_or("/usr/local".to_string());

    let binary_dir = get_current_binary_directory();

    let _ = write_to_file(
        &binary_dir.join("libalpm")
                  .join("90-dracut-efibin-install.hook"),
        &PacmanInstallHook{ prefix: prefix.clone() } as &dyn ToString
    );
    let _ = write_to_file(
        &binary_dir.join("libalpm")
                  .join("90-dracut-efibin-clean.hook"),
        &PacmanCleanHook{ prefix: prefix.clone() } as &dyn ToString
    );
}

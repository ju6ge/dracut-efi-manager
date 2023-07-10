use std::{collections::BTreeMap, fs, process::Command, path::Path};

use config::Config;
use regex::Regex;
use serde::{Serialize, Deserialize};
use spinners::{Spinner, Spinners};

#[derive(Debug, Serialize, Deserialize)]
struct EfiStubBuildConfig {
    kernel_modules_dir: String,
    efi_dir: String,

    build_mappings: BTreeMap<String, String>
}

#[derive(Debug, Clone)]
struct KernelVersion {
    version: String,
    full_name: String
}

fn main() {
    let settings: EfiStubBuildConfig = Config::builder()
                                              .add_source(config::File::with_name("settings.toml"))
                                              .build_cloned()
                                              .expect("Error Reading Config File!")
                                              .try_deserialize()
                                              .expect("Error parsing Configuration!");

    // compile reqex patterns for kernel
    let kernel_regex: BTreeMap<&String, Regex> = BTreeMap::from_iter(
        settings.build_mappings.keys().map(|v| {
            (v, Regex::new(v).unwrap())
        })
    );

    //accumulator for kernels modules directories to find the newest fill with empty vectors
    let mut found_kernel_modules: BTreeMap<&String, Vec<KernelVersion>> = BTreeMap::from_iter(
        settings.build_mappings.keys().map(|v| { (v, Vec::new()) })
    );

    // cluster kernels by version
    for entry in fs::read_dir(settings.kernel_modules_dir).unwrap() {
        match entry {
            Ok(entry) => {
                let kernel_folder = entry.file_name();
                for (kernel, regex) in kernel_regex.iter() {
                    match regex.captures(kernel_folder.to_str().unwrap()) {
                        Some(captures) => {
                            found_kernel_modules.get_mut(kernel).and_then(|vec| {
                                vec.push(KernelVersion {
                                    version: captures.get(1).unwrap().as_str().to_string(),
                                    full_name: captures.get(0).unwrap().as_str().to_string()
                                });
                                Some(())
                            });
                        },
                        None => { /* does not match nothing to do here */ },
                    }
                }
            }
            Err(_) => { /* nothing to do here */ }
        }
    }
    //println!("{found_kernel_modules:#?}");

    //find the newest kernel for each release type
    let newest_kernels = found_kernel_modules.into_iter().filter_map(|(k, v)| {
        if v.len() > 1 {
            let mut newest = v.first().unwrap().clone();
            for kernel in v {
                if version_operators::Version::from_str(&newest.version) < version_operators::Version::from_str(&kernel.version) {
                    newest = kernel.clone();
                }
            }
            Some((k, newest.full_name.clone()))
        } else if v.len() == 1 {
           Some((k, v.first().unwrap().full_name.clone()))
        } else {
            None
        }
    }).collect::<BTreeMap<&String, String>>();

    for kernel in newest_kernels {
        let version = kernel.1;
        let destination = Path::new(&settings.efi_dir).join(settings.build_mappings.get(kernel.0).expect("Error getting stub destination from config!"));
        let mut task_indicator = Spinner::new(Spinners::Dots9, format!("Building efi-stub for kernel {version} at {destination:?}"));
        let dracut_build = Command::new("dracut")
                                   .args(["--force", "--uefi", "--uefi-stub", "/usr/lib/systemd/boot/efi/linuxx64.efi.stub", destination.to_str().unwrap(), "--kver", &version])
                                   .output();
        match dracut_build {
            Ok(_result) => {
                task_indicator.stop_with_symbol("✅");
            },
            Err(_err) => {
                task_indicator.stop_with_symbol("❌");
            },
        }
    }
}

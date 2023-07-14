//! Dracut Stub Manager
//!
//! A tool to create EFI binaries for Archlinux kernels for direct boot without a bootloader.
use std::{collections::BTreeMap, fs, path::Path, process::Command, io::{self, Write}};

use config::Config;
use regex::Regex;
use serde::{Deserialize, Serialize};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, about, version)]
#[command( help_template = "Author: {author} \nVersion: {version} \n{about-section} \n{usage-heading} {usage}\n\n{all-args} {tab}" )]
struct DracutCmdArgs {
    #[command(subcommand)]
    command: DracutBuilderCommands
}

#[derive(Debug, Clone, Parser)]
enum DracutBuilderCommands {
    /// build efi binaries for all configured kernels
    Build,
    /// clean efi directory from kernels that are not required anymore
    Clean
}

#[derive(Debug, Serialize, Deserialize)]
struct EfiStubBuildConfig {
    kernel_modules_dir: String,
    efi_dir: String,

    build_mappings: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct KernelVersion {
    version: String,
    full_name: String,
}

impl Into<KernelVersion> for &dyn ToString {
    fn into(self) -> KernelVersion {
        let version_regex = Regex::new(r"[\d]+\.[\d]+\.[\d]+").unwrap();
        let subversion_regex = Regex::new(r"-([\d]+)") .unwrap();

        let to_parse = self.to_string();
        let version = version_regex.captures(&to_parse)
                                .and_then(|v| v.get(0))
                                .and_then(|v| Some(v.as_str()))
                                .unwrap_or("0.0.0");
        let subversion = subversion_regex.captures(&to_parse)
                                .and_then(|v| v.get(1))
                                .and_then(|v| Some(v.as_str()))
                                .unwrap_or("0");
        KernelVersion { version: format!("{version}.{subversion}"), full_name: to_parse }
    }
}

/// check if the modules directory contains a linux image or is a leftover from upgrades/ùninstalls
fn is_valid_installation(modules_path: &Path) -> bool {
    modules_path.join("vmlinuz").exists()
}


fn get_newest_installed_kernels(settings: &EfiStubBuildConfig) -> BTreeMap<&String, String> {
    //accumulator for kernels modules directories to find the newest fill with empty vectors
    let mut found_kernel_modules: BTreeMap<&String, Vec<KernelVersion>> =
        BTreeMap::from_iter(settings.build_mappings.keys().map(|v| (v, Vec::new())));

    // cluster kernels by version
    for entry in fs::read_dir(settings.kernel_modules_dir.clone()).unwrap() {
        match entry {
            Ok(entry) => {
                if is_valid_installation(&entry.path()) {
                    let kernel_folder = entry.file_name();
                    for kernel_ident in settings.build_mappings.keys() {
                        match kernel_folder.clone().into_string() {
                            Ok(kernel_folder_name) => {
                                if kernel_folder_name.contains(kernel_ident) {
                                    found_kernel_modules.get_mut(kernel_ident).unwrap().push((&kernel_folder_name as &dyn ToString).into());
                                } else {
                                    continue;
                                }
                            },
                            Err(_) => continue,
                        }
                    }
                }
            }
            Err(_) => { /* nothing to do here */ }
        }
    }
    //println!("{found_kernel_modules:#?}");

    //find the newest kernel for each release type
    let newest_kernels = found_kernel_modules
        .into_iter()
        .filter_map(|(k, v)| {
            if v.len() > 1 {
                let mut newest = v.first().unwrap().clone();
                for kernel in v {
                    if version_operators::Version::from_str(&newest.version)
                        < version_operators::Version::from_str(&kernel.version)
                    {
                        newest = kernel.clone();
                    }
                }
                Some((k, newest.full_name.clone()))
            } else if v.len() == 1 {
                Some((k, v.first().unwrap().full_name.clone()))
            } else {
                None
            }
        })
        .collect::<BTreeMap<&String, String>>();

        //println!("{newest_kernels:?}");
        newest_kernels
}

fn build_efi_binaries(settings: &EfiStubBuildConfig) {
    for kernel in get_newest_installed_kernels(&settings) {
        let version = kernel.1;
        let destination = Path::new(&settings.efi_dir).join(
            settings
                .build_mappings
                .get(kernel.0)
                .expect("Error getting binary destination from config!"),
        );
        print!("Building efi binary for kernel {version} at {} … ", destination.file_name().unwrap().to_str().unwrap());
        let _ = io::stdout().flush();
        let dracut_build = Command::new("dracut")
            .args([
                "--force",
                "--uefi",
                "--uefi-stub",
                "/usr/lib/systemd/boot/efi/linuxx64.efi.stub",
                destination.to_str().unwrap(),
                "--kver",
                &version,
            ])
            .output();
        match dracut_build {
            Ok(result) => {
                if result.status.success() {
                    println!("✅");
                } else {
                    println!("❌");
                }
            }
            Err(_err) => {
                println!("❌");
            }
        }
    }
}

fn clean_efi_binaries(settings: &EfiStubBuildConfig) {
    let mut removed_binarys = 0;
    let installed_kernels = get_newest_installed_kernels(&settings);
    for (configured_kernel, destination_name) in settings.build_mappings.iter() {
        // check if configured kernel is installed
        if !installed_kernels.contains_key(configured_kernel) {
            removed_binarys += 1;
            //if not check if there still is an efi binary present and if so remove it
            let destination = Path::new(&settings.efi_dir).join(destination_name);
            if destination.exists() {
                print!("Removing old efi binary for {configured_kernel} kernel at {destination_name} … ");
                let _ = io::stdout().flush();
                let remove_old_binary = Command::new("rm").arg(destination.to_str().unwrap()).output();
                match remove_old_binary {
                    Ok(result) => {
                        if result.status.success() {
                            println!("✅");
                        } else {
                            println!("❌");
                        }
                    }
                    Err(_err) => {
                        println!("❌");
                    }
                }
            }
        }
    }
    if removed_binarys == 0 {
        println!("Efi directory is already clean.");
    }
}

#[cfg(debug_assertions)]
const SETTINGS_FILE: &str = "settings.toml";

#[cfg(not(debug_assertions))]
const SETTINGS_FILE: &str = "/etc/dracut-efi-manager.toml";

fn main() {
    let args = DracutCmdArgs::parse();

    let settings: EfiStubBuildConfig = Config::builder()
        .add_source(config::File::with_name(SETTINGS_FILE))
        .build_cloned()
        .expect("Error Reading Config File!")
        .try_deserialize()
        .expect("Error parsing Configuration!");

    match args.command {
        DracutBuilderCommands::Build => {
            build_efi_binaries(&settings)
        },
        DracutBuilderCommands::Clean => {
            clean_efi_binaries(&settings)
        },
    }

}

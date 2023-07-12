//! Dracut Stub Manager
//!
//! A tool to create EFI binaries for Archlinux kernels for direct boot without a bootloader.
use std::{collections::BTreeMap, fs, path::Path, process::Command};

use config::Config;
use regex::Regex;
use serde::{Deserialize, Serialize};
use spinners::{Spinner, Spinners};
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

fn get_newest_installed_kernels(settings: &EfiStubBuildConfig) -> BTreeMap<&String, String> {
    let version_regex = Regex::new(r"[\d]+\.[\d]+\.[\d]+").unwrap();
    let subversion_regex = Regex::new(r"-([\d]+)") .unwrap();

    //accumulator for kernels modules directories to find the newest fill with empty vectors
    let mut found_kernel_modules: BTreeMap<&String, Vec<KernelVersion>> =
        BTreeMap::from_iter(settings.build_mappings.keys().map(|v| (v, Vec::new())));

    // cluster kernels by version
    for entry in fs::read_dir(settings.kernel_modules_dir.clone()).unwrap() {
        match entry {
            Ok(entry) => {
                let kernel_folder = entry.file_name();
                for kernel_ident in settings.build_mappings.keys() {
                    match kernel_folder.clone().into_string() {
                        Ok(kernel_folder_name) => {
                            if kernel_folder_name.contains(kernel_ident) {
                                let version = version_regex.captures(&kernel_folder_name)
                                                           .and_then(|v| v.get(0))
                                                           .and_then(|v| Some(v.as_str()))
                                                           .unwrap_or("0.0.0");
                                let subversion = subversion_regex.captures(&kernel_folder_name)
                                                           .and_then(|v| v.get(1))
                                                           .and_then(|v| Some(v.as_str()))
                                                           .unwrap_or("0");
                                found_kernel_modules.get_mut(kernel_ident).unwrap().push(KernelVersion{ version: format!("{version}.{subversion}"),full_name: kernel_folder_name })
                            } else {
                                continue;
                            }
                        },
                        Err(_) => continue,
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
                .expect("Error getting stub destination from config!"),
        );
        let mut task_indicator = Spinner::new(
            Spinners::Dots9,
            format!("Building efi-stub for kernel {version} at {destination:?}"),
        );
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
            Ok(_result) => {
                task_indicator.stop_with_symbol("✅");
            }
            Err(_err) => {
                task_indicator.stop_with_symbol("❌");
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
                let mut task_indicator = Spinner::new(
                    Spinners::Dots9,
                    format!("Removing old efi binary for {configured_kernel} kernel! => {destination_name}"),
                );
                let remove_old_binary = Command::new("rm").arg(destination.to_str().unwrap()).output();
                match remove_old_binary {
                    Ok(_result) => {
                        task_indicator.stop_with_symbol("✅");
                    }
                    Err(_err) => {
                        task_indicator.stop_with_symbol("❌");
                    }
                }
            }
        }
    }
    if removed_binarys == 0 {
        println!("Efi directory is already clean.");
    }
}

fn main() {
    let args = DracutCmdArgs::parse();

    let settings: EfiStubBuildConfig = Config::builder()
        .add_source(config::File::with_name("settings.toml"))
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

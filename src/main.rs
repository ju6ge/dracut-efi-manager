//! Dracut Stub Manager
//!
//! A tool to create EFI binaries for Archlinux kernels for direct boot without a bootloader.
use std::{
    collections::BTreeMap,
    fmt::Display,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

use clap::Parser;
use config::Config;
use efivar::boot::{BootEntry, BootEntryAttributes, EFIHardDrive, FilePath, FilePathList};
use gpt::{partition::Partition, partition_types};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(author, about, version)]
#[command(
    help_template = "Author: {author} \nVersion: {version} \n{about-section} \n{usage-heading} {usage}\n\n{all-args} {tab}"
)]
struct DracutCmdArgs {
    #[command(subcommand)]
    command: DracutBuilderCommands,
}

#[derive(Debug, Clone, Parser)]
enum DracutBuilderCommands {
    /// build efi binaries for all configured kernels
    Build,
    /// clean efi directory from kernels that are not required anymore
    Clean,
    /// scan drives for efi partions and add boot entries for efi executables
    Bootentries,
    /// interactive boot order manipulation
    Bootorder,
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
        let subversion_regex = Regex::new(r"-([\d]+)").unwrap();

        let to_parse = self.to_string();
        let version = version_regex
            .captures(&to_parse)
            .and_then(|v| v.get(0))
            .and_then(|v| Some(v.as_str()))
            .unwrap_or("0.0.0");
        let subversion = subversion_regex
            .captures(&to_parse)
            .and_then(|v| v.get(1))
            .and_then(|v| Some(v.as_str()))
            .unwrap_or("0");
        KernelVersion {
            version: format!("{version}.{subversion}"),
            full_name: to_parse,
        }
    }
}

/// check if the modules directory contains a linux image or is a leftover from upgrades/ùninstalls
fn is_valid_installation(modules_path: &Path) -> bool {
    modules_path.join("vmlinuz").exists()
}

fn get_current_running_kernel() -> String {
    let current_running_kernel: String =
        String::from_utf8(Command::new("uname").arg("-r").output().unwrap().stdout)
            .unwrap()
            .trim()
            .to_string();
    current_running_kernel
}

fn get_newest_installed_kernels(settings: &EfiStubBuildConfig) -> BTreeMap<&String, String> {
    //accumulator for kernels modules directories to find the newest fill with empty vectors
    let mut found_kernel_modules: BTreeMap<&String, Vec<KernelVersion>> =
        BTreeMap::from_iter(settings.build_mappings.keys().map(|v| (v, Vec::new())));

    // cluster kernels by version
    for entry in fs::read_dir(settings.kernel_modules_dir.clone()).unwrap() {
        entry.ok().and_then(|entry| {
            if is_valid_installation(&entry.path()) {
                let kernel_folder = entry.file_name();
                for kernel_ident in settings.build_mappings.keys() {
                    kernel_folder
                        .clone()
                        .into_string()
                        .ok()
                        .and_then(|kernel_folder_name| {
                            if kernel_folder_name.contains(kernel_ident) {
                                found_kernel_modules
                                    .get_mut(kernel_ident)
                                    .unwrap()
                                    .push((&kernel_folder_name as &dyn ToString).into());
                            }
                            Some(())
                        });
                }
            }
            Some(())
        });
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
        print!(
            "Building efi binary for kernel {version} at {} … ",
            destination.file_name().unwrap().to_str().unwrap()
        );
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
                let remove_old_binary = Command::new("rm")
                    .arg(destination.to_str().unwrap())
                    .output();
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
    //cleanup old kernel directories
    for entry in fs::read_dir(settings.kernel_modules_dir.clone()).unwrap() {
        entry.ok().and_then(|entry| {
            entry
                .file_name()
                .into_string()
                .ok()
                .and_then(|kernel_name| {
                    if kernel_name != get_current_running_kernel()
                        && !is_valid_installation(&entry.path())
                    {
                        print!("Removing old kernel modules directory {kernel_name} … ");
                        let _ = io::stdout().flush();
                        let remove_old_kernel_module_dir = Command::new("rm")
                            .args(["-rf", entry.path().to_str().unwrap()])
                            .output();
                        match remove_old_kernel_module_dir {
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
                    Some(())
                })
        });
    }
}

fn boot_entries_handler() {
    let efi_partitions = get_efi_partitions();
    if efi_partitions.is_empty() {
        println!("No efi partitions found. No boot entries to configure.");
    } else {
        for efi_part in efi_partitions {
            let efi_binaries = efi_part.get_efi_binaries();
            let exisiting_boot_entries = efi_part.existing_boot_entries();
            for efi_bin in efi_binaries {
                if !exisiting_boot_entries.contains_key(&efi_bin) {
                    if dialoguer::Confirm::new()
                        .with_prompt(format!(
                            "No boot entry found for efi binary `{:?}`. Do you want to create one?",
                            efi_bin.as_path()
                        ))
                        .interact()
                        .unwrap()
                    {
                        let description: String = dialoguer::Input::new()
                            .with_prompt("Give the boot Entry a description:")
                            .interact()
                            .unwrap();
                        add_boot_entry(efi_part.gen_boot_entry(&efi_bin, description), None);
                    }
                }
            }
        }
    }
}

#[cfg(debug_assertions)]
const SETTINGS_FILE: &str = "settings.toml";

#[cfg(not(debug_assertions))]
const SETTINGS_FILE: &str = "/etc/dracut-efi-manager.toml";

fn get_disk_device_paths() -> Vec<PathBuf> {
    let mut disks = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/class/block") {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                let file_name = path.file_name().unwrap().to_string_lossy().to_string();

                let partition_file = path.join("partition");

                if path.is_dir()
                    && file_name != "."
                    && file_name != ".."
                    && !partition_file.exists()
                {
                    disks.push(Path::new("/dev").join(file_name))
                }
            }
        }
    }
    disks
}

fn get_mount_dir(device: &Path) -> Option<PathBuf> {
    if let Ok(mounts) = fs::read_to_string("/proc/mounts") {
        for line in mounts.lines() {
            let line_split = line.split(' ').collect::<Vec<_>>();
            if let Some(mounted_device) = line_split.get(0) {
                if Path::new(mounted_device) == device {
                    return Some(Path::new(line_split.get(1).unwrap()).to_path_buf());
                }
            }
        }
    }
    None
}

struct EfiPartionInfo {
    part_nr: u32,
    disk_device: PathBuf,
    info: Partition,
}

impl EfiPartionInfo {
    fn get_partiton_device(&self) -> Option<PathBuf> {
        let disk_name = self
            .disk_device
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        if let Ok(entries) = fs::read_dir(Path::new("/sys/class/block").join(disk_name)) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    let file_name = path.file_name().unwrap().to_string_lossy().to_string();

                    let partition_file = path.join("partition");

                    if path.is_dir()
                        && file_name != "."
                        && file_name != ".."
                        && partition_file.exists()
                    {
                        if let Ok(mut partition_file) = File::open(partition_file) {
                            let mut num_str = String::new();
                            let _ = partition_file.read_to_string(&mut num_str);
                            if let Ok(nr) = num_str.trim().parse::<u32>() {
                                if nr == self.part_nr {
                                    return Some(Path::new("/dev").join(file_name));
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn get_efi_binaries(&self) -> Vec<PathBuf> {
        let mut efi_binaries = Vec::new();
        if let Some(partition_device) = self.get_partiton_device() {
            let mut had_to_be_mounted = false;
            let mount_dir = match get_mount_dir(&partition_device) {
                Some(path) => path,
                None => {
                    had_to_be_mounted = true;
                    let temp_mount_dir = create_temp_mount_dir().unwrap();

                    let _ = Command::new("mount")
                        .args([partition_device.as_os_str(), temp_mount_dir.as_os_str()])
                        .output();
                    temp_mount_dir
                }
            };
            efi_binaries.append(
                &mut get_efi_binaries(&mount_dir)
                    .iter_mut()
                    .map(|efi_bin_path| {
                        efi_bin_path.strip_prefix(&mount_dir).unwrap().to_path_buf()
                    })
                    .collect(),
            );

            if had_to_be_mounted {
                let _ = Command::new("umount")
                    .args([mount_dir.as_os_str()])
                    .output();
                fs::remove_dir_all(&mount_dir).unwrap();
            }
        }
        efi_binaries
    }

    fn existing_boot_entries(&self) -> BTreeMap<PathBuf, BootEntry> {
        let mut boot_entries_map = BTreeMap::new();
        if let Ok(boot_entries) = efivar::system().get_boot_entries() {
            for entry in boot_entries {
                if let Ok(entry) = entry.0 {
                    if let Some(boot_path) = entry.entry.clone().file_path_list {
                        for efi_bin in self.get_efi_binaries() {
                            let mut boot_file_path = boot_path
                                .file_path
                                .path
                                .to_string_lossy()
                                .to_string()
                                .replace("\\", "/");
                            if boot_file_path.starts_with("/") {
                                boot_file_path = boot_file_path.replacen("/", "", 1);
                            }
                            if boot_path.hard_drive.partition_sig == self.info.part_guid
                                && boot_file_path == efi_bin.to_string_lossy().to_string()
                            {
                                boot_entries_map.insert(efi_bin, entry.entry.clone());
                            }
                        }
                    }
                }
            }
        }
        boot_entries_map
    }

    fn gen_boot_entry(&self, efi_bin: &Path, name: String) -> BootEntry {
        BootEntry {
            attributes: BootEntryAttributes::LOAD_OPTION_ACTIVE,
            description: name,
            file_path_list: Some(FilePathList {
                file_path: FilePath {
                    path: Path::new(&efi_bin.to_string_lossy().to_string().replace("/", "\\"))
                        .to_path_buf(),
                },
                hard_drive: EFIHardDrive {
                    partition_number: self.part_nr,
                    partition_start: self.info.first_lba,
                    partition_size: (self.info.last_lba + 1) - self.info.first_lba,
                    partition_sig: self.info.part_guid,
                    format: 2,
                    sig_type: efivar::boot::EFIHardDriveType::Gpt,
                },
            }),
            optional_data: Vec::new(),
        }
    }
}

fn create_temp_mount_dir() -> io::Result<PathBuf> {
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_else(|_| 0);

    let temp_dir_name = format!("temp_efimount_{}", unique_id);
    let temp_dir_path = Path::new("/tmp").join(&temp_dir_name);

    fs::create_dir(&temp_dir_path)?;

    Ok(temp_dir_path)
}

fn get_efi_binaries(path: &Path) -> Vec<PathBuf> {
    let mut binaries = Vec::new();
    if path.is_dir() {
        binaries.append(
            &mut fs::read_dir(path)
                .and_then(|entries| {
                    Ok(entries
                        .into_iter()
                        .map(|entry| {
                            if let Ok(entry) = entry {
                                let file_name =
                                    entry.file_name().as_os_str().to_string_lossy().to_string();
                                if file_name != "." && file_name != ".." {
                                    Some(get_efi_binaries(&entry.path()))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .flatten()
                        .flatten()
                        .collect::<Vec<PathBuf>>())
                })
                .unwrap(),
        );
    } else {
        if let Some(ext) = path.extension() {
            if ext.eq_ignore_ascii_case("efi") {
                binaries.push(path.to_path_buf());
            }
        }
    }
    binaries
}

fn get_efi_partitions() -> Vec<EfiPartionInfo> {
    let mut efi_partitions = Vec::new();
    for disk in get_disk_device_paths() {
        if let Ok(gpt_info) = gpt::disk::read_disk(&disk) {
            for (nr, part) in gpt_info.partitions().into_iter() {
                if part.part_type_guid == partition_types::EFI {
                    efi_partitions.push(EfiPartionInfo {
                        part_nr: *nr,
                        disk_device: disk.clone(),
                        info: part.clone(),
                    });
                }
            }
        }
    }
    efi_partitions
}

fn add_boot_entry(entry: BootEntry, boot_position: Option<usize>) {
    if let Ok(mut boot_order) = efivar::system().get_boot_order() {
        let boot_id = get_free_boot_id(&boot_order);
        let _ = efivar::system().add_boot_entry(boot_id, entry);
        match boot_position {
            Some(boot_position) => boot_order.insert(boot_position, boot_id),
            None => boot_order.push(boot_id),
        }
        let _ = efivar::system().set_boot_order(boot_order);
    }
}

fn get_free_boot_id(boot_order: &Vec<u16>) -> u16 {
    let mut numbers = boot_order.clone();
    numbers.sort();
    let mut result = 0;
    for nr in numbers {
        if nr == result {
            result = nr + 1;
        }
    }
    result
}

#[cfg(test)]
mod boot_nr_gen_tests {
    use crate::get_free_boot_id;

    #[test]
    fn gen_boot_number_test() {
        assert_eq!(get_free_boot_id(&vec![1, 2]), 0);
        assert_eq!(get_free_boot_id(&vec![0, 2]), 1);
        assert_eq!(get_free_boot_id(&vec![0, 10, 11, 1]), 2);
    }
}

struct BootOrderData {
    id: u16,
    name: String,
}

impl Display for BootOrderData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

fn main() {
    let args = DracutCmdArgs::parse();

    let settings: Option<EfiStubBuildConfig> = Config::builder()
        .add_source(config::File::with_name(SETTINGS_FILE))
        .build_cloned()
        .and_then(|settings_file| settings_file.try_deserialize())
        .ok();

    match args.command {
        DracutBuilderCommands::Build => {
            if let Some(settings) = settings {
                build_efi_binaries(&settings)
            } else {
                eprintln!("Build configuration not found!");
            }
        }
        DracutBuilderCommands::Clean => {
            if let Some(settings) = settings {
                clean_efi_binaries(&settings)
            } else {
                eprintln!("Build configuration not found!");
            }
        }
        DracutBuilderCommands::Bootentries => {
            boot_entries_handler();
        }
        DracutBuilderCommands::Bootorder => {
            if let Ok(boot_order) = efivar::system().get_boot_order() {
                if let Ok(boot_id_map) =
                    efivar::system()
                        .get_boot_entries()
                        .and_then(|boot_entries| {
                            Ok(boot_entries
                                .into_iter()
                                .map(|entry| {
                                    if let Ok(boot_entry) = entry.0 {
                                        Some((boot_entry.id, boot_entry.entry.description))
                                    } else {
                                        None
                                    }
                                })
                                .flatten()
                                .collect::<BTreeMap<u16, String>>())
                        })
                {
                    let boot_order_names: Vec<BootOrderData> = boot_order
                        .into_iter()
                        .map(|id| {
                            let name = boot_id_map.get(&id).unwrap();
                            BootOrderData {
                                id,
                                name: name.to_string(),
                            }
                        })
                        .collect();
                    let new_order = dialoguer::Sort::new()
                        .with_prompt("What boot order do you prefer? … Use space to select and arrow keys to navigate")
                        .items(&boot_order_names)
                        .interact()
                        .unwrap();
                    let new_boot_order: Vec<u16> = new_order
                        .into_iter()
                        .map(|pos| boot_order_names.get(pos).unwrap())
                        .map(|b| b.id)
                        .collect();

                    let _ = efivar::system().set_boot_order(new_boot_order);
                }
            }
        }
    }
}

extern crate std;
extern crate zip;
extern crate config;
extern crate fs_extra;
extern crate regex;
extern crate remove_dir_all;
extern crate glob;

use types::*;

use APP_INFO;
use app_dirs::*;

use std::io::prelude::*;
use std::iter::Iterator;
use std::io::{Write, Seek};

use std::path::{Path, PathBuf};
use std::fs::File;

use zip::result::ZipError;
use zip::write::FileOptions;
use walkdir::{WalkDir, DirEntry};
use remove_dir_all::*;

use glob::glob;

static mut IS_LOVE_BUILT: bool = false;

/// Get the folder name of where a version of LÖVE is stored in the app cache
pub fn get_love_version_file_name(version: &LoveVersion, platform: &Platform, bitness: &Bitness) -> String {
    match (version, platform, bitness) {
        (&LoveVersion::V11_2,   &Platform::Windows, &Bitness::X64) => "love-11.2-win64",
        (&LoveVersion::V11_2,   &Platform::Windows, &Bitness::X86) => "love-11.2-win32",
        (&LoveVersion::V11_2,   &Platform::MacOs,   _)             => "love.app",

        (&LoveVersion::V11_1,   &Platform::Windows, &Bitness::X64) => "love-11.1-win64",
        (&LoveVersion::V11_1,   &Platform::Windows, &Bitness::X86) => "love-11.1-win32",
        (&LoveVersion::V11_1,   &Platform::MacOs,   _)             => "love.app",

        (&LoveVersion::V11_0,   &Platform::Windows, &Bitness::X64) => "love-11.0.0-win64",
        (&LoveVersion::V11_0,   &Platform::Windows, &Bitness::X86) => "love-11.0.0-win32",
        (&LoveVersion::V11_0,   &Platform::MacOs,   _)             => "love.app",

        (&LoveVersion::V0_10_2, &Platform::Windows, &Bitness::X64) => "love-0.10.2-win64",
        (&LoveVersion::V0_10_2, &Platform::Windows, &Bitness::X86) => "love-0.10.2-win32",
        (&LoveVersion::V0_10_2, &Platform::MacOs,   _)             => "love.app",
    }.to_owned()
}

/// Get file name of the .love file (same for all platforms)
pub fn get_love_file_name(project: &Project) -> String {
    format!("{}.love", project.title.to_owned())
}

/// Get file name for individual binary based on platform and bitness
pub fn get_output_filename(project: &Project, platform: &Platform, bitness: &Bitness) -> String {
    match (platform, bitness) {
        (&Platform::Windows, &Bitness::X64) => format!("{}.exe", project.package_name),
        (&Platform::Windows, &Bitness::X86) => format!("{}.exe", project.package_name),
        (&Platform::MacOs,   _) =>             format!("{}.app", project.title),
    }
}

/// Get file name of the distributed .zip file based on platform and bitness
pub fn get_zip_output_filename(project: &Project, platform: &Platform, bitness: &Bitness) -> String {
    match (platform, bitness) {
        (&Platform::Windows, &Bitness::X64) => format!("{}-win64", project.title),
        (&Platform::Windows, &Bitness::X86) => format!("{}-win32", project.title),
        (&Platform::MacOs,   _) =>             format!("{}-macos", project.title),
    }
}

/// Get a platform-specific path to the app cache directory where LÖVE is stored.
pub fn get_love_version_path(version: &LoveVersion, platform: &Platform, bitness: &Bitness) -> PathBuf {
    let filename = get_love_version_file_name(version, platform, bitness);

    // @DoNotFix: The forward slash here is intentional. It will get escaped by
    // get_app_dir automatically to match the OS preference.
    let subdirectory = format!("{}/{}", &version.to_string(), &filename);
    get_app_dir(AppDataType::UserData, &APP_INFO, &subdirectory).unwrap()
}

// TODO: check CONFIG to see if DEBUG set to true should halt building process
pub fn scan_files(project: &Project, build_settings: &BuildSettings) {
    let globals_file = format!("{1}{0}{2}", std::path::MAIN_SEPARATOR, project.directory, "globals.lua");
    println!("Looking for globals.lua at: {}", globals_file);

    let mut f = File::open(globals_file).expect("file not found");

    let mut contents = String::new();
    f.read_to_string(&mut contents)
        .expect("something went wrong reading the file");

    if (contents.find("RELEASE = false") != None && contents.find("DEBUG = not RELEASE") != None) || contents.find("DEBUG = true") != None {
        println!("!!!WARNING!!! Debug is ENABLED!");
        if build_settings.debug_halt {
            panic!("DEBUG set to false. If you want to build anyway, modify debug_halt in Settings.");
        }
    } else if (contents.find("RELEASE = true") != None && contents.find("DEBUG = not RELEASE") != None) || contents.find("DEBUG = false") != None {
        println!("You can rest easy. Debug is DISABLED.")
    } else {
        println!("It is uncertain what DEBUG is set to. Make sure to verify it on your own.")
    }
}

pub fn build_init(project: &Project, build_settings: &BuildSettings) {
    scan_files(&project, &build_settings);

    // Ensure release directory exists.
    let release_dir_path = project.get_release_path();

    if !release_dir_path.exists() {
        println!("Creating release directory {}", release_dir_path.display());

        match std::fs::create_dir(&release_dir_path) {
            Ok(_) => {},
            Err(err) => panic!("Could not create release directory: '{}'", err)
        };
    }
}

//
// LÖVE .love build
//
pub fn build_love(project: &Project, build_settings: &BuildSettings) {
    let method = zip::CompressionMethod::Deflated;

    let src_dir = &project.directory;
    let love_path = project.get_release_path().join(get_love_file_name(&project));
    let dst_file = love_path.to_str().unwrap();
    println!("Outputting LÖVE as {}", dst_file);

    match collect_zip_directory(src_dir, dst_file, method, &build_settings.ignore_list) {
        Ok(_) => {
            println!("done: {} written to {}", src_dir, dst_file);
        },
        Err(e) => {
            println!("Error: {:?}", e);
        }
    }

    unsafe {
        IS_LOVE_BUILT = true;
    }
}

//
// Windows .exe build
//
pub fn build_windows(project: &Project, build_settings: &BuildSettings, version: &LoveVersion, bitness: &Bitness) {
    unsafe {
        if !IS_LOVE_BUILT {
            println!("Error: Cannot build for windows because .love not built.");
        }
    }

    let app_dir_path = get_love_version_path(version, &Platform::Windows, bitness);

    let mut app_dir_path_clone = PathBuf::new();
    app_dir_path_clone.clone_from(&app_dir_path);

    let mut love_exe_path = PathBuf::from(app_dir_path);
    love_exe_path.push("love.exe");
    if !love_exe_path.exists() {
        println!("\nlove.exe not found at '{}'\nYou may need to download LÖVE first: `love-kit download {}`", love_exe_path.display(), version.to_string());
        panic!();
    }

    let exe_file_name = get_output_filename(project, &Platform::Windows, bitness);
    let zip_output_file_name = &get_zip_output_filename(project, &Platform::Windows, bitness);
    let mut output_path = project.get_release_path();
    output_path.push(zip_output_file_name);

    println!("Removing existing directory {}", output_path.display());
    if output_path.exists() {
        match std::fs::remove_dir_all(&output_path) {
            Ok(_) => {},
            Err(err) => panic!("Could not remove directory: '{}'", err)
        };
    }

    // Create temp directory to be zipped and removed later
    match std::fs::create_dir(&output_path) {
        Ok(_) => {},
        Err(err) => panic!("Could not create build directory: '{}'", err)
    };

    output_path.push(exe_file_name);

    println!("Copying love from {}", love_exe_path.display());

    println!("Outputting exe to {}", output_path.display());
    let mut output_file = match File::create(&output_path) {
        Ok(file) => file,
        Err(why) => {
            panic!("Unable to create file `{}`: {}", output_path.display(), why);
        }
    };

    let love_file_name = get_love_file_name(&project);
    let mut local_love_file_path = PathBuf::from(project.get_release_path());
    local_love_file_path.push(love_file_name);

    println!("Copying project .love from {}", local_love_file_path.display());

    let mut copy_options = fs_extra::file::CopyOptions::new();
    copy_options.overwrite = true;

    // copy all .dll, .txt, and .ico files from the love source
    let search_for_files_dll = app_dir_path_clone.join("*.dll");
    let search_for_files_txt = app_dir_path_clone.join("*.txt");
    let search_for_files_ico = app_dir_path_clone.join("*.ico");
    for entry in glob(search_for_files_dll.to_str().unwrap()).unwrap().chain(
                 glob(search_for_files_txt.to_str().unwrap()).unwrap()).chain(
                 glob(search_for_files_ico.to_str().unwrap()).unwrap()) {
        match entry {
            Ok(path) => {
                let local_file_name = path.file_name().unwrap().to_str().unwrap();
                //println!("Local file name: {}", local_file_name);
                //println!("copying {:?} to {}", path.display(), project.get_release_path().join(zip_output_file_name).join(local_file_name).display());

                match fs_extra::file::copy(&path, &project.get_release_path().join(zip_output_file_name).join(local_file_name), &copy_options) {
                    Ok(_) => {},
                    Err(err) => panic!("{:?}", err)
                };
            },

            // if the path matched but was unreadable,
            // thereby preventing its contents from matching
            Err(e) => println!("{:?}", e),
        }
    }

    let paths = &[
        love_exe_path.as_path(),
        local_love_file_path.as_path(),
    ];

    let mut buffer = Vec::new();
    for path in paths {
        if path.is_file() {
            let mut file = match File::open(path) {
                Ok(file) => file,
                Err(why) => panic!("Could not open file: {}", why),
            };

            match file.read_to_end(&mut buffer) {
                Ok(_) => {},
                Err(why) => panic!("Could not read file: {}", why),
            };

            match output_file.write_all(&*buffer) {
                Ok(_) => {},
                Err(why) => panic!("Could not write output file: {}", why),
            };

            buffer.clear();
        }
    }

    // Time to zip up the whole directory
    let zip_output_file_name = get_zip_output_filename(project, &Platform::Windows, bitness);
    let output_path = project.get_release_path().join(zip_output_file_name);

    let src_dir = output_path.clone();
    let src_dir = src_dir.to_str().unwrap();

    let mut dst_file = output_path.clone();
    dst_file.set_extension("zip");
    let dst_file = dst_file.to_str().unwrap();
    
    let method = zip::CompressionMethod::Deflated;
    let ignore_list: &Vec<String> = &vec!();
    match collect_zip_directory(src_dir, dst_file, method, ignore_list) {
        Ok(_) => {
            println!("done: {} written to {}", src_dir, dst_file);
        },
        Err(e) => {
            println!("Error: {:?}", e);
        }
    }
    let path = PathBuf::new().join(src_dir);
    println!("Removing {}", path.display());
    match remove_dir_all(&path) {
        Ok(_) => {},
        Err(err) => panic!("{:?}", err)
    };
}

//
// macOS .app build
//
pub fn build_macos(project: &Project, build_settings: &BuildSettings, version: &LoveVersion, bitness: &Bitness) {
    unsafe {
        if !IS_LOVE_BUILT {
            println!("Error: Cannot build for macOS because .love not built.");
        }
    }

    let love_path = get_love_version_path(version, &Platform::MacOs, bitness);
    if !love_path.exists() {
        println!("\nLÖVE not found at '{}'\nYou may need to download LÖVE first: `love-kit download {}`", love_path.display(), version.to_string());
        panic!();
    }

    let output_file_name = get_output_filename(project, &Platform::MacOs, bitness);
    let output_path = project.get_release_path();
    let mut final_output_path = PathBuf::from(project.get_release_path());
    final_output_path.push(output_file_name);

    println!("Copying LÖVE from {} to {}", love_path.display(), output_path.display());

    let mut copy_options = fs_extra::dir::CopyOptions::new();
    copy_options.overwrite = true;
    match fs_extra::dir::copy(&love_path, &output_path, &copy_options) {
        Ok(_) => {},
        Err(err) => panic!("{:?}", err)
    };

    let mut local_love_app_path = PathBuf::from(project.get_release_path());
    local_love_app_path.push(love_path.file_name().unwrap().to_str().unwrap());

    if final_output_path.exists() {
        println!("Removing {}", final_output_path.display());
        match std::fs::remove_dir_all(&final_output_path) {
            Ok(_) => {},
            Err(err) => panic!("{:?}", err)
        };
    }

    println!("Renaming LÖVE from {} to {}", local_love_app_path.display(), final_output_path.display());
    match std::fs::rename(&local_love_app_path, &final_output_path) {
        Ok(_) => {},
        Err(err) => panic!("{:?}", err)
    };

    let love_file_name = get_love_file_name(&project);
    let mut local_love_file_path = PathBuf::from(project.get_release_path());
    local_love_file_path.push(love_file_name);
    let mut resources_path = PathBuf::from(&final_output_path);
    resources_path.push("Contents");
    resources_path.push("Resources");
    resources_path.push("love_file_name");
    println!("Copying .love file from {} to {}", local_love_file_path.display(), resources_path.display());

    let mut copy_options = fs_extra::file::CopyOptions::new();
    copy_options.overwrite = true;
    match fs_extra::file::copy(local_love_file_path, resources_path, &copy_options) {
        Ok(_) => {},
        Err(err) => panic!("{:?}", err)
    };

    // Rewrite plist file
    let mut plist_path = PathBuf::from(&final_output_path);
    plist_path.push("Contents");
    plist_path.push("Info.plist");

    println!("Rewriting {}", plist_path.display());

    let mut buffer = String::new();
    let mut file = match std::fs::OpenOptions::new()
        .read(true)
        .open(&plist_path) {
        Ok(file) => file,
        Err(why) => panic!("Could not open file: {}", why),
    };

    match file.read_to_string(&mut buffer) {
        Ok(_) => {},
        Err(why) => panic!("Could not read file: {}", why),
    };

    let re = regex::Regex::new("(CFBundleIdentifier.*\n\t<string>)(.*)(</string>)").unwrap();
    buffer = re.replace(buffer.as_str(), |caps: &regex::Captures| {
        [&caps[1], project.uti.as_str(), &caps[3]].join("")
    }).to_string();

    let re = regex::Regex::new("(CFBundleName.*\n\t<string>)(.*)(</string>)").unwrap();
    buffer = re.replace(buffer.as_str(), |caps: &regex::Captures| {
        [&caps[1], project.title.as_str(), &caps[3]].join("")
    }).to_string();

    let re = regex::RegexBuilder::new("^\t<key>UTExportedTypeDeclarations.*(\n.*)+\t</array>\n")
        .multi_line(true)
        .build()
        .unwrap();
    buffer = re.replace(buffer.as_str(), "").to_string();

    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&plist_path) {
        Ok(file) => file,
        Err(why) => panic!("Could not open file: {}", why),
    };

    match file.write_all(buffer.as_bytes()) {
        Ok(_) => {},
        Err(why) => panic!("Could not write output file: {}", why),
    };
}

fn should_exclude_file(file_name: String, ignore_list: &Vec<String>) -> bool {
    for exclude_name in ignore_list {
        if file_name.find(exclude_name) != None {
            return true;
        }
    }

    false
}

fn zip_directory<T>(it: &mut Iterator<Item=DirEntry>, prefix: &str, writer: T, method: zip::CompressionMethod, ignore_list: &Vec<String>)
              -> zip::result::ZipResult<()>
    where T: Write+Seek
{
    let mut zip = zip::ZipWriter::new(writer);
    let options = FileOptions::default()
        .compression_method(method)
        .unix_permissions(0o644);

    let mut buffer = Vec::new();
    for entry in it {
        let path = entry.path();
        let name = path.strip_prefix(Path::new(prefix))
            .unwrap()
            .to_str()
            .unwrap();

        if path.is_file() && !should_exclude_file(name.to_string(), &ignore_list) {
            println!("adding {:?} ...", name);
            zip.start_file(name, options)?;
            let mut f = File::open(path)?;

            f.read_to_end(&mut buffer)?;
            zip.write_all(&*buffer)?;
            buffer.clear();
        }
    }
    zip.finish()?;
    Result::Ok(())
}

fn collect_zip_directory(src_dir: &str, dst_file: &str, method: zip::CompressionMethod, ignore_list: &Vec<String>) -> zip::result::ZipResult<()> {
    if !Path::new(src_dir).is_dir() {
        return Err(ZipError::FileNotFound);
    }

    let path = Path::new(dst_file);
    let file = File::create(&path).unwrap();

    let walkdir = WalkDir::new(src_dir.to_string());
    let it = walkdir.into_iter();

    zip_directory(&mut it.filter_map(|e| e.ok()), src_dir, file, method, ignore_list)?;

    Ok(())
}

impl Project {
    fn get_release_path(&self) -> PathBuf {
        let mut path = Path::new(self.directory.as_str())
            .canonicalize()
            .unwrap();
        path.push("release");
        path
    }
}
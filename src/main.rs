use core::panic;
use fern::Dispatch;
use log::{info, warn};
use rayon::prelude::*;
use std::env;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
// use walkdir::DirEntry;
use bincode;
use chrono::Local;
use hex;
use image::io::Reader as ImageReader;
use jwalk::WalkDirGeneric;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsStr;
use std::hash::BuildHasherDefault;
use trash;
use xxhash_rust::xxh3::xxh3_64;
use xxhash_rust::xxh3::Xxh3;
use zip::write::FileOptions;
use zip::CompressionMethod::Stored;
use zip::ZipWriter;
type XxHashMap<K, V> = HashMap<K, V, BuildHasherDefault<Xxh3>>;
use std::time::Instant;
fn setup_logger() -> Result<(), fern::InitError> {
    let file_log = Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} [{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .chain(fern::log_file("output.log")?);

    let console_log = Dispatch::new()
        .level(log::LevelFilter::Info)
        .chain(io::stdout());

    Dispatch::new().chain(file_log).chain(console_log).apply()?;

    Ok(())
}

fn make_zip(
    folder_path: &PathBuf,
    zip_options: FileOptions,
    pack_files_list: Vec<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let zip_path = folder_path.with_extension("zip");
    let mut zip = ZipWriter::new(File::create(&zip_path)?);

    for pack_file in &pack_files_list {
        // file_name = sub_entry.file_name().to_str().unwrap().to_string();
        let f = File::open(pack_file);
        let mut file = match f {
            Ok(file) => file,
            Err(err) => {
                if err.kind() == std::io::ErrorKind::NotFound {
                    warn!(
                        "aleady delete. maybe duplicate file: {}",
                        pack_file.display()
                    );
                    continue;
                }
                warn!("Failed to open file: {}", pack_file.display());
                panic!("Failed to open file: {}", pack_file.display());
            }
        };

        let mut buffer = vec![];
        file.read_to_end(&mut buffer)?;
        zip.start_file(
            pack_file
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default(),
            zip_options,
        )?;
        zip.write_all(&mut buffer)?;
        fs::remove_file(&pack_file)?; //zip 파일에 추가한 jxl 파일을 삭제.
    }
    zip.finish()?;
    //생성한 zip file이 비었다면 삭제.
    if pack_files_list.is_empty() {
        trash::delete(&zip_path)?;
    }
    Ok(())
}

fn image_to_jxl(
    exe_path: &PathBuf,
    image_path: &PathBuf,
    jxl_path: &PathBuf,
    png_args: &Vec<String>,
    jpg_args: &Vec<String>,
    image_format: &image::ImageFormat,
) -> Result<(), String> {
    //jxl 파일이 이미 존재하면, 파일 해시 확인.
    let mut new_jxl_path = jxl_path.to_owned();
    let mut number = 0;
    loop {
        //중복 파일 검사
        if new_jxl_path.exists() {
            info!("Same name file exists: {}", &new_jxl_path.display());
            number += 1;

            new_jxl_path = new_jxl_path.with_file_name(format!(
                "{}({}).jxl",
                jxl_path
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .unwrap_or_default(),
                number
            ));
        } else {
            break;
        }
    }

    let mut command = Command::new(exe_path);
    command.args([image_path, &new_jxl_path]);
    let output = match image_format {
        image::ImageFormat::Jpeg => command.args(jpg_args).output().expect("Failed to convert"),
        image::ImageFormat::Png => command.args(png_args).output().expect("Failed to convert"),
        _ => {
            return Err(format!(
                "Failed file: {} \nerror message: Not supported file type",
                image_path.display(),
            ));
        }
    };

    if !output.status.success() {
        return Err(format!(
            "Failed file: {} \nerror message: {}",
            image_path.display(),
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                format!("stdout: {}\nstderr: {}", stdout, stderr)
            }
        ));
    }

    //jxl 파일이 이미 존재하면, 파일 해시를 확인해 일치하는 파일은 삭제..
    if number > 0 {
        //let new_jxl_hash = finalize_md5(&new_jxl_path)?.finalize();
        let new_jxl_hash = finalize_xxhash(&new_jxl_path)
            .map_err(|err| {
                format!(
                    "Failed to finalize_xxhash: {}\nerror message: {}",
                    new_jxl_path.display(),
                    err
                )
            })
            .unwrap();

        for i in (0..number).rev() {
            let compare_path = if i == 0 {
                jxl_path.to_owned()
            } else {
                jxl_path.with_file_name(format!(
                    "{}({}).jxl",
                    jxl_path
                        .file_stem()
                        .and_then(OsStr::to_str)
                        .unwrap_or_default(),
                    i
                ))
            };
            let compare_jxl_hash = finalize_xxhash(&compare_path)
                .map_err(|err| {
                    format!(
                        "Failed to finalize_xxhash: {}\nerror message: {}",
                        compare_path.display(),
                        err
                    )
                })
                .unwrap();

            if new_jxl_hash != compare_jxl_hash {
                warn!(
            "Hash of converted file is different from original file. Please check the file: {}",
            jxl_path.to_string_lossy()
            ); //파일 해시가 다르면, 파일을 삭제하지 않음.
            } else {
                info!("file is same so delete: {}", &compare_path.display());
                fs::remove_file(&compare_path).expect("Failed to delete file");
            }
        }
        fs::rename(&new_jxl_path, jxl_path).expect("Failed to rename file");
    }

    Ok(())
}

fn finalize_xxhash(file_path: &PathBuf) -> Result<u64, String> {
    let mut file = File::open(file_path).expect("Failed to open file");
    let mut buffer = vec![];
    file.read_to_end(&mut buffer).expect("Failed to read file");
    Ok(xxh3_64(&buffer))
}

fn is_image_file(path: &PathBuf) -> Result<image::ImageFormat, String> {
    let file_ext = path
        .extension()
        .expect("hey, i don't get ext")
        .to_str()
        .expect("hey, i don't get ext str")
        .to_lowercase();

    if ["jpg", "png", "jpeg"].contains(&file_ext.as_str()) {
        //이미지 포맷 확인
        let img_format = ImageReader::open(path)
            .expect("Failed to open image file")
            .format()
            .expect("Failed to read image format");

        match img_format {
            image::ImageFormat::Png | image::ImageFormat::Jpeg => Ok(img_format),
            // Add more formats as needed
            _ => Err(format!(
                "Failed file: {}\nWarn: This file is skip",
                path.display(),
            )), //"The image is current not support format"
        }
    } else if "jxl" == file_ext.as_str() {
        Err(format!(
            "Failed file: {}\nWarn: This file is skip",
            path.display()
        )) //"The file is already jxl"
    } else {
        Err(format!(
            "Failed file: {}\nError: The file is not image",
            path.display()
        )) //"The file is not image"
    }
}
//TODO
//1. 따옴표가 없는 줄도 경로 알아채기. - 보류.
//2. 폴더 스캔에 걸리는 시간 측정. - Done.
//3. 작업 내역을 생성 및 저장 및 읽기로 이미 작업을 마친 폴더는 건너뛰기. - Done.
//4. md5 해시가 아닌, xxhash로 이미지 해시값 생성. - Done.
fn main() -> Result<(), Box<dyn Error>> {
    setup_logger()?;
    //cjxl_args 불러오기.
    let cjxl_args = match read_cjxl_args() {
        Ok(jxl_args) => {
            if jxl_args.png_args.is_empty() || jxl_args.jpg_args.is_empty() {
                //cjxl_args.ini가 비어있으면, 프로그램 종료.
                warn!("cjxl_args.ini is empty");
                panic!("cjxl_args.ini is empty");
            }
            jxl_args
        }
        Err(_) => {
            let mut text_file = File::create("cjxl_args.ini")?;
            text_file.write_all(b"delete_folder=false\ndelete_source_image=false\nmake_zip=true\ndont_use_trashcan_just_delete=false\npng_args=[--distance=0,--effort=7]\njpg_args=[--distance=0,--effort=9,--lossless_jpeg=1]")?;

            warn!("Failed to read cjxl_args.ini");
            let mut jxl_args = JxlArgs::default();
            jxl_args.png_args = vec!["--distance=0".to_string(), "--effort=7".to_string()];
            jxl_args.jpg_args = vec![
                "--distance=0".to_string(),
                "--effort=9".to_string(),
                "--lossless_jpeg=1".to_string(),
            ];
            jxl_args
        }
    };

    info!(
        "{}",
        format!(
            r#"cjxl_args:
            delete_folder={}
            delete_source_image={}
            make_zip={}
            dont_use_trashcan_just_delete={}
            png_args={:?}
            jpg_args={:?}"#,
            cjxl_args.delete_folder_plag,
            cjxl_args.delete_source_image_plag,
            cjxl_args.make_zip_plag,
            cjxl_args.dont_use_trashcan_just_delete,
            cjxl_args.png_args,
            cjxl_args.jpg_args
        )
    );

    //let current_dir = env::current_dir().unwrap();
    let exe_path = env::current_dir()?.join("cjxl.exe");
    if !exe_path.exists() {
        warn!("cjxl.exe not exists");
        panic!("cjxl.exe not exists");
    }
    info!("current cjxl.exe location: {:?}", exe_path);
    let begin_args: Vec<String> = std::env::args().collect();
    let path_pattern = ['"', '\'']; //윈도우에서는 "로 경로를 감싸는 경우가 많아서, "를 제거함. 따옴표도 제거.
    let folder_path_input = if begin_args.len() > 1 {
        begin_args[1].clone() // 첫번째 인자로 폴더 경로를 받음.
    } else {
        info!("Drag&Drop folder to convert jxl and changed zip you want: ");
        let mut user_input = String::new();
        io::stdout().flush()?;

        loop {
            io::stdin().read_line(&mut user_input)?;
            if user_input.is_empty() {
                warn!("Folder path is empty. Please enter again: ");
                continue;
            }

            //let folder_path_input = user_input.trim().trim_matches(path_pattern).to_string();
            let folder_path_input = path_pattern.into_iter().find_map(|c| {
                let split: Vec<&str> = user_input.split(c).collect();
                if split.len() > 1 && !split[1].is_empty() {
                    Some(split[1].to_string())
                } else {
                    None
                }
            });
            match folder_path_input {
                Some(path) if PathBuf::from(&path).is_dir() => {
                    break path;
                }
                _ => {
                    warn!("Folder path is not valid. Please enter again: ");
                    user_input.clear();
                }
            }
        }
    };

    let work_folder_hex = hex::encode(xxh3_64(folder_path_input.as_bytes()).to_be_bytes());

    //작업 내역이 있으면, 작업 내역을 불러옴.
    let mut work_info = match File::open(format!("{}.bin", work_folder_hex)) {
        Ok(file) => {
            info!("Worklist file exists so load worklist.");
            bincode::deserialize_from(file).expect("Failed to load worklist")
        }
        Err(_) => {
            info!("No worklist file so create new worklist.");
            WorkInfo::new(folder_path_input.clone(), cjxl_args.clone())
        }
    };

    //작업 시간 측정
    let start = Instant::now();

    //폴더 경로를 가져옴
    // let folder_path_input = user_input.trim().split('"').nth(1).unwrap_or_default();
    //폴더 경로를 PathBuf로 변환
    // let mother_folder_path = PathBuf::from(user_input.trim().split('"').collect::<Vec<&str>>()[1]);

    // ...
    let mut folder_list = WalkDirGeneric::<(usize, bool)>::new(&folder_path_input)
        .process_read_dir(|depth, path, read_dir_state, children| {
            //작업 내역이 있으면, 작업 내역을 불러옴.
            children.retain(|dir_entry_result| {
                dir_entry_result.as_ref().map_or(false, |dir_entry| {
                    if dir_entry.path().is_dir() {
                        true
                    } else {
                        false
                    }
                })
            });
        })
        .into_iter()
        .filter_map(|dir_result| match dir_result {
            Ok(entry) => {
                let path = entry.path();
                match work_info.worklist.get(&path) {
                    Some(&true) => {
                        info!("Already done: {}", path.display());
                        None
                    }
                    Some(&false) => Some(path),
                    None => {
                        work_info.worklist.insert(path.clone(), false);
                        Some(path)
                    }
                }
            }
            Err(_) => None,
        })
        .collect::<Vec<PathBuf>>();

    let file_search_duration = start.elapsed();
    let (fs_hours, fs_minutes, fs_seconds, fs_milliseconds) = time_display(file_search_duration);
    info!(
        "File search time:{:02}:{:02}:{:02}:{:03}",
        fs_hours, fs_minutes, fs_seconds, fs_milliseconds
    );

    //하위 폴더부터 변환하기 위해 폴더 리스트를 정렬.
    folder_list.sort_by(|a, b| {
        //components().count()는 폴더의 깊이를 나타냄.
        b.components().count().cmp(&a.components().count())
    });

    let zip_options = FileOptions::default()
        .compression_method(Stored)
        .unix_permissions(0o755);

    //if folder not in imagefile, then skip
    //폴더 안에 이미지 파일을 찾아서 jxl로 변환하는 놈
    for folder_path in folder_list {
        info!("folder_path: {}", folder_path.display());

        let mut delete_folder_plag = cjxl_args.delete_folder_plag;
        let mut can_i_make_zip_file = cjxl_args.make_zip_plag;

        let pack_files_list: Vec<Result<JXL, String>> = PathBuf::from(&folder_path)
            .read_dir()?
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_file() && entry.path().extension().unwrap() != "zip")
            .collect::<Vec<_>>()
            .par_iter()
            .map(|entry| match is_image_file(&entry.path()) {
                Ok(image_format) => {
                    let jxl_path = entry.path().with_extension("jxl");
                    match image_to_jxl(
                        &exe_path,
                        &entry.path(),
                        &jxl_path,
                        &cjxl_args.png_args,
                        &cjxl_args.jpg_args,
                        &image_format,
                    ) {
                        Ok(_) => {
                            if cjxl_args.delete_source_image_plag {
                                //원본 파일 삭제.
                                if cjxl_args.dont_use_trashcan_just_delete {
                                    fs::remove_file(&entry.path()).expect("Failed to delete file");
                                } else {
                                    trash::delete(&entry.path()).expect("Failed to delete file33");
                                }
                            }
                            Ok(JXL::Converted(jxl_path))
                        }
                        Err(err) => Err(err),
                    }
                }
                Err(err) => {
                    if err.contains("This file is skip") {
                        Ok(JXL::ExistFromBegin(entry.path()))
                    } else {
                        Err(err)
                    }
                }
            })
            .collect();

        if pack_files_list.is_empty() {
            info!("No image file in folder");
            work_info.update_list_element(&folder_path);
            continue;
        }

        //error check
        for pack_file in &pack_files_list {
            if let Err(err) = pack_file {
                warn!("{}\npass this folder", err);
                can_i_make_zip_file = false;
                delete_folder_plag = false;
                break;
            }
        }

        if !can_i_make_zip_file {
            //파일 하나라도 이미지 변환에 실패하는 경우, zip 파일을 만들지 않음.
            //또는 사용자가 zip 파일을 만들지 않겠다고 설정한 경우.
            info!("Do not make zip file");
            work_info.update_list_element(&folder_path);
            continue;
        }

        //zip 파일 만들기 - png, jpg를 포함한 모든 이미지 파일을 zip으로 묶음.
        let pack_files_list: Vec<_> = pack_files_list
            .into_iter()
            .filter_map({
                |pack_file| match pack_file {
                    Ok(JXL::Converted(jxl_path)) => Some(jxl_path),
                    Ok(JXL::ExistFromBegin(jxl_path)) => Some(jxl_path),
                    Err(err) => {
                        //not use this line, but still keep it.
                        warn!("{}\npass this folder", err);
                        None
                    }
                }
            })
            .collect();
        let _ = make_zip(&folder_path, zip_options, pack_files_list);

        if delete_folder_plag {
            //폴더 삭제. 만약 삭제하려는 폴더 안에 다른 폴더, 이미지가 아닌 파일이 있으면 폴더를 삭제하지 않음.
            if cjxl_args.dont_use_trashcan_just_delete {
                fs::remove_dir_all(&folder_path)?;
            } else {
                trash::delete(&folder_path)?;
            }
        }

        //work_info.worklist[&folder_path] = true;//why rust not allow IndexMut????
        //work_info 업데이트
        work_info.update_list_element(&folder_path);
    }

    //작업 리스트 저장.
    let mut file = File::create(format!("{}.bin", work_folder_hex))?;
    bincode::serialize_into(&mut file, &work_info)?;

    info!("All done!");
    let duration_time = start.elapsed();
    let (hours, minutes, seconds, milliseconds) = time_display(duration_time);
    info!(
        "Duration time:{:02}:{:02}:{:02}.{:03}",
        hours, minutes, seconds, milliseconds
    );
    info!(
        "File search time:{:02}:{:02}:{:02}.{:03}",
        fs_hours, fs_minutes, fs_seconds, fs_milliseconds
    );

    println!("Press Enter to exit...");
    io::stdin().read_line(&mut String::new())?;
    Ok(())
}

fn read_cjxl_args() -> Result<JxlArgs, String> {
    let content = fs::read_to_string("cjxl_args.ini");
    let args_pattern: &[_] = &['[', ']'];
    let mut jxlargs = JxlArgs {
        delete_folder_plag: false,
        delete_source_image_plag: false,
        make_zip_plag: true,
        dont_use_trashcan_just_delete: false,
        png_args: vec![],
        jpg_args: vec![],
    };
    match content {
        Ok(file) => {
            for line in file.lines() {
                let arg = line.trim();
                if let Some(args_str) = arg.strip_prefix("delete_folder=") {
                    if args_str.to_lowercase() == "true" {
                        jxlargs.delete_folder_plag = true;
                    }
                } else if let Some(args_str) = arg.strip_prefix("delete_source_image=") {
                    if args_str.to_lowercase() == "true" {
                        jxlargs.delete_source_image_plag = true;
                    }
                } else if let Some(args_str) = arg.strip_prefix("make_zip=") {
                    if args_str.to_lowercase() == "false" {
                        jxlargs.make_zip_plag = false;
                    }
                } else if let Some(args_str) = arg.strip_prefix("dont_use_trashcan_just_delete=") {
                    if args_str.to_lowercase() == "true" {
                        jxlargs.dont_use_trashcan_just_delete = true;
                    }
                } else if let Some(args_str) = arg.strip_prefix("png_args=") {
                    jxlargs.png_args = args_str
                        .trim_matches(args_pattern)
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                } else if let Some(args_str) = arg.strip_prefix("jpg_args=") {
                    jxlargs.jpg_args = args_str
                        .trim_matches(args_pattern)
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
            }
        }
        Err(_) => {
            return Err("Failed to open file".to_string());
        }
    }
    Ok(jxlargs)
}

fn time_display(duration_time: std::time::Duration) -> (u128, u128, u128, u128) {
    let milliseconds = duration_time.as_millis();
    let hours = milliseconds / 3_600_000;
    let minutes = milliseconds % 3_600_000 / 60_000;
    let seconds = milliseconds % 60_000 / 1_000;
    let milliseconds = milliseconds % 1_000;
    (hours, minutes, seconds, milliseconds)
}

#[derive(Serialize, Deserialize, Clone)]
struct JxlArgs {
    delete_folder_plag: bool,
    delete_source_image_plag: bool,
    make_zip_plag: bool,
    dont_use_trashcan_just_delete: bool,
    png_args: Vec<String>,
    jpg_args: Vec<String>,
}
impl Default for JxlArgs {
    fn default() -> Self {
        JxlArgs {
            delete_folder_plag: false,
            delete_source_image_plag: false,
            make_zip_plag: true,
            dont_use_trashcan_just_delete: false,
            png_args: vec![],
            jpg_args: vec![],
        }
    }
}

enum JXL {
    ExistFromBegin(PathBuf),
    Converted(PathBuf),
}

#[derive(Serialize, Deserialize)]
struct WorkInfo {
    work_folder_path: String,
    work_setting: JxlArgs,
    worklist: XxHashMap<PathBuf, bool>,
}
impl WorkInfo {
    fn new(work_folder_path: String, work_setting: JxlArgs) -> Self {
        WorkInfo {
            work_folder_path,
            work_setting,
            worklist: XxHashMap::default(),
        }
    }

    fn update_list_element(&mut self, folder_path: &PathBuf) {
        if let Some(value) = self.worklist.get_mut(folder_path) {
            *value = true;
        } else {
            panic!("Failed to update worklist");
        }
    }
}

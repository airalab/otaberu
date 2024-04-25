use std::env;
use std::fs;
use std::path::PathBuf;
use tracing::error;
use tracing::info;

fn get_home_dir() -> String {
    match env::var_os("HOME") {
        Some(val) => val.to_str().unwrap().to_string(),
        None => return String::from(""),
    }
}

pub fn get_merklebot_data_path() -> String {
    let path = get_home_dir() + "/.merklebot/";
    path
}

pub fn get_job_data_path(job_id: &str) -> String {
    let path = get_merklebot_data_path() + "job-" + job_id + "/";
    path
}

pub fn create_job_data_dir(job_id: &str) -> std::io::Result<String> {
    let path = get_job_data_path(job_id);
    fs::create_dir_all(path.clone())?;
    Ok(path)
}

pub fn get_files_in_directory_recursively(path: &str) -> Result<Vec<PathBuf>, walkdir::Error> {
    let mut result_files: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(path) {
        match entry {
            Ok(entry) => match entry.metadata() {
                Ok(md) => {
                    if md.is_file() {
                        result_files.push(entry.path().to_path_buf());
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    Ok(result_files)
}

pub async fn upload_content(
    robot_server_url: String,
    path: PathBuf,
    key: String,
    job_id: String,
    api_key: String,
) {
    let splitted_path: Vec<&str> = key.split("/").collect();
    let filename = splitted_path.last().unwrap().to_owned();
    info!("splitted path: {:?}", splitted_path);
    let client = reqwest::Client::new();
    let url = reqwest::Url::parse(&(robot_server_url + "/upload_content")).unwrap();
    info!("{:?}", url);
    let file_fs = fs::read(path).unwrap();
    let part = reqwest::multipart::Part::bytes(file_fs).file_name(filename.to_owned());
    let form = reqwest::multipart::Form::new()
        .text("key", key)
        .text("job_id", job_id)
        .text("api_key", api_key)
        .part("file", part);

    match client.post(url).multipart(form).send().await {
        Ok(res) => {
            let res_txt = res.text().await.unwrap_or("no message".to_string());
            info!("content upload res: {:?}", res_txt);
        }
        Err(_) => {
            error!("Error while uploading content");
        }
    }
}

use std::path::Path;
use tokio::fs::{self, File};
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use anyhow::{Result, Context};

pub async fn store_file<P: AsRef<Path>>(path: P, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent).await
            .context("Failed to create parent directories")?;
    }

    let mut file = File::create(path).await
        .context("Failed to create file")?;
    
    file.write_all(data).await
        .context("Failed to write data to file")?;
    
    Ok(())
}

pub async fn read_file<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
    let mut file = File::open(path).await
        .context("Failed to open file")?;
        
    let mut contents = Vec::new();
    file.read_to_end(&mut contents).await
        .context("Failed to read file contents")?;
        
    Ok(contents)
}

/// Checks if a file exists asynchronously
pub async fn file_exists<P: AsRef<Path>>(path: P) -> bool {
    fs::metadata(path).await.is_ok()
}

pub async fn delete_file<P: AsRef<Path>>(path: P) -> Result<()> {
    if file_exists(&path).await {
        fs::remove_file(path).await
            .context("Failed to delete file")?;
    }
    Ok(())
}

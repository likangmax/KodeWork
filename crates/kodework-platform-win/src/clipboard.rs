//! Native system clipboard ingestion for terminal paste workflows.
//!
//! The renderer is intentionally not allowed to read arbitrary local files.
//! This module accepts only a small, auditable set of image/PDF formats and
//! returns validated absolute paths for the existing SFTP pipeline.

use clipboard_rs::{common::RustImage, Clipboard, ClipboardContext};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_ASSETS: usize = 16;
const MAX_ASSET_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
static NEXT_ASSET_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClipboardAsset {
    pub path: PathBuf,
    pub extension: String,
    pub temporary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ClipboardPayload {
    Text(String),
    Assets(Vec<ClipboardAsset>),
    Empty,
}

/// Writes a terminal selection to the native system clipboard. Keeping this
/// in the native layer avoids WebView clipboard-permission inconsistencies.
pub fn write_text(text: &str) -> Result<(), String> {
    const MAX_SELECTION_BYTES: usize = 4 * 1024 * 1024;
    if text.len() > MAX_SELECTION_BYTES {
        return Err("终端选区过大，不能超过 4 MiB".into());
    }
    let context = ClipboardContext::new().map_err(|error| error.to_string())?;
    context
        .set_text(text.to_owned())
        .map_err(|error| error.to_string())
}

/// Reads the native clipboard. File drops take precedence over a bitmap,
/// while text is used only when no supported binary asset is present.
pub fn read_clipboard(temp_root: &Path) -> Result<ClipboardPayload, String> {
    let context = ClipboardContext::new().map_err(|error| error.to_string())?;
    if let Ok(files) = context.get_files() {
        if !files.is_empty() {
            return validated_assets(files.into_iter().map(PathBuf::from));
        }
    }

    if let Ok(image) = context.get_image() {
        std::fs::create_dir_all(temp_root)
            .map_err(|error| format!("无法创建剪贴板暂存目录：{error}"))?;
        let path = temp_root.join(unique_name("clipboard", "png"));
        image
            .save_to_path(path.to_string_lossy().as_ref())
            .map_err(|error| format!("无法保存剪贴板图片：{error}"))?;
        return Ok(ClipboardPayload::Assets(vec![ClipboardAsset {
            path,
            extension: "png".into(),
            temporary: true,
        }]));
    }

    match context.get_text() {
        Ok(text) if !text.is_empty() => Ok(ClipboardPayload::Text(text)),
        _ => Ok(ClipboardPayload::Empty),
    }
}

fn validated_assets(paths: impl IntoIterator<Item = PathBuf>) -> Result<ClipboardPayload, String> {
    let mut assets = Vec::new();
    let mut total = 0_u64;
    for path in paths {
        if assets.len() >= MAX_ASSETS {
            return Err(format!("一次最多粘贴 {MAX_ASSETS} 个文件"));
        }
        if !path.is_absolute() {
            return Err("剪贴板文件路径必须是绝对路径".into());
        }
        let metadata = path
            .metadata()
            .map_err(|error| format!("无法读取剪贴板文件：{error}"))?;
        if !metadata.is_file() {
            return Err("剪贴板项目不是普通文件".into());
        }
        if metadata.len() > MAX_ASSET_BYTES {
            return Err("单个剪贴板文件不能超过 512 MiB".into());
        }
        total = total
            .checked_add(metadata.len())
            .ok_or_else(|| "剪贴板文件总大小溢出".to_string())?;
        if total > MAX_TOTAL_BYTES {
            return Err("剪贴板文件总大小不能超过 1 GiB".into());
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| "只支持 PDF 和常见图片格式".to_string())?;
        validate_magic(&path, &extension)?;
        assets.push(ClipboardAsset {
            path,
            extension,
            temporary: false,
        });
    }
    Ok(ClipboardPayload::Assets(assets))
}

fn validate_magic(path: &Path, extension: &str) -> Result<(), String> {
    let mut header = [0_u8; 16];
    let mut file = File::open(path).map_err(|error| format!("无法打开剪贴板文件：{error}"))?;
    let bytes_read = file
        .read(&mut header)
        .map_err(|error| format!("无法验证剪贴板文件：{error}"))?;
    let header = &header[..bytes_read];
    let valid = match extension {
        "pdf" => header.starts_with(b"%PDF-"),
        "png" => header.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" => header.starts_with(&[0xff, 0xd8, 0xff]),
        "gif" => header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a"),
        "bmp" => header.starts_with(b"BM"),
        "tif" | "tiff" => header.starts_with(b"II*\0") || header.starts_with(b"MM\0*"),
        "webp" => header.len() >= 12 && header.starts_with(b"RIFF") && &header[8..12] == b"WEBP",
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err("剪贴板仅接受内容真实匹配的 PDF、PNG、JPEG、GIF、BMP、TIFF 或 WebP".into())
    }
}

fn unique_name(prefix: &str, extension: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    let sequence = NEXT_ASSET_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{timestamp}-{sequence}.{extension}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_validation_rejects_renamed_executable() {
        let root = std::env::temp_dir().join(unique_name("kodework-clipboard-test", "dir"));
        std::fs::create_dir_all(&root)
            .unwrap_or_else(|error| unreachable!("temporary directory: {error}"));
        let path = root.join("malware.pdf");
        std::fs::write(&path, b"MZ not a pdf")
            .unwrap_or_else(|error| unreachable!("test file: {error}"));
        assert!(validated_assets([path]).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn magic_validation_accepts_pdf() {
        let root = std::env::temp_dir().join(unique_name("kodework-clipboard-test", "dir"));
        std::fs::create_dir_all(&root)
            .unwrap_or_else(|error| unreachable!("temporary directory: {error}"));
        let path = root.join("document.pdf");
        std::fs::write(&path, b"%PDF-1.7\n")
            .unwrap_or_else(|error| unreachable!("test file: {error}"));
        let payload =
            validated_assets([path]).unwrap_or_else(|error| unreachable!("valid PDF: {error}"));
        assert!(matches!(payload, ClipboardPayload::Assets(value) if value.len() == 1));
        let _ = std::fs::remove_dir_all(root);
    }
}

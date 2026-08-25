#![forbid(unsafe_code)]

//! In-memory SFTP server for offline integration tests: a tiny virtual
//! file system (files + directories) served over the russh-sftp server
//! protocol. Supports the operations the transfer manager uses: open,
//! read, write, close, stat, opendir/readdir, remove, rename, mkdir,
//! rmdir, realpath.

use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Packet, Status, StatusCode, Version,
};
use russh_sftp::server::{Handler as SftpHandler, StatusReply};
use russh_sftp::{de, extensions};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Virtual file system content served by the fake SFTP server.
/// Paths are absolute (`/a/b`); `~/x` is treated as `/x`.
#[derive(Debug, Clone, Default)]
pub struct FakeSftpContent {
    pub files: HashMap<String, Vec<u8>>,
    pub dirs: Vec<String>,
}

#[derive(Clone)]
enum SftpHandleState {
    File { path: String, write: bool },
    Dir { entries: Vec<File>, exhausted: bool },
}

/// russh-sftp server handler backed by an in-memory file system.
pub struct InMemorySftp {
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    dirs: Arc<Mutex<HashSet<String>>>,
    handles: Mutex<HashMap<String, SftpHandleState>>,
    next_handle: AtomicU64,
}

fn normalize(path: &str) -> String {
    let path = if let Some(rest) = path.strip_prefix("~/") {
        format!("/{rest}")
    } else {
        path.to_string()
    };
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

impl InMemorySftp {
    #[must_use]
    pub fn new(content: FakeSftpContent) -> Self {
        let files = content.files;
        let mut dirs: HashSet<String> = content.dirs.into_iter().map(|d| normalize(&d)).collect();
        dirs.insert("/".to_string());
        Self {
            files: Arc::new(Mutex::new(files)),
            dirs: Arc::new(Mutex::new(dirs)),
            handles: Mutex::new(HashMap::new()),
            next_handle: AtomicU64::new(1),
        }
    }

    fn attrs_for(&self, path: &str) -> Result<FileAttributes, StatusReply> {
        let path = normalize(path);
        if let Ok(files) = self.files.lock() {
            if let Some(content) = files.get(&path) {
                let mut attrs = FileAttributes {
                    size: Some(content.len() as u64),
                    ..FileAttributes::default()
                };
                attrs.set_regular(true);
                return Ok(attrs);
            }
        }
        if let Ok(dirs) = self.dirs.lock() {
            if dirs.contains(&path) {
                let mut attrs = FileAttributes {
                    size: Some(0),
                    ..FileAttributes::default()
                };
                attrs.set_dir(true);
                return Ok(attrs);
            }
        }
        Err(StatusReply::new(StatusCode::NoSuchFile))
    }

    fn list_dir(&self, path: &str) -> Result<Vec<File>, StatusReply> {
        let path = normalize(path);
        let dirs = self
            .dirs
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?;
        if !dirs.contains(&path) {
            return Err(StatusReply::new(StatusCode::NoSuchFile));
        }
        let files = self
            .files
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?;
        let prefix = if path == "/" {
            String::new()
        } else {
            format!("{path}/")
        };
        let mut names = HashSet::new();
        let mut out: Vec<File> = Vec::new();
        for (file_path, content) in files.iter() {
            if let Some(rest) = file_path.strip_prefix(&prefix) {
                if rest.contains('/') || rest.is_empty() {
                    continue;
                }
                if names.insert(rest.to_string()) {
                    let mut attrs = FileAttributes {
                        size: Some(content.len() as u64),
                        ..FileAttributes::default()
                    };
                    attrs.set_regular(true);
                    out.push(File::new(rest.to_string(), attrs));
                }
            }
        }
        for dir in dirs.iter() {
            if let Some(rest) = dir.strip_prefix(&prefix) {
                if rest.contains('/') || rest.is_empty() {
                    continue;
                }
                if names.insert(rest.to_string()) {
                    let mut attrs = FileAttributes::default();
                    attrs.set_dir(true);
                    out.push(File::new(rest.to_string(), attrs));
                }
            }
        }
        out.sort_by(|a, b| a.filename.cmp(&b.filename));
        Ok(out)
    }
}

impl SftpHandler for InMemorySftp {
    type Error = StatusReply;

    fn unimplemented(&self) -> Self::Error {
        StatusReply::new(StatusCode::OpUnsupported)
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        let mut version = Version::new();
        version
            .extensions
            .insert(extensions::EXPAND_PATH.to_string(), "1".to_string());
        Ok(version)
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let path = normalize(&filename);
        let mut files = self
            .files
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?;
        let exists = files.contains_key(&path);
        if pflags.contains(OpenFlags::READ) && !exists && !pflags.contains(OpenFlags::CREATE) {
            return Err(StatusReply::new(StatusCode::NoSuchFile));
        }
        if pflags.contains(OpenFlags::CREATE) && !exists {
            files.insert(path.clone(), Vec::new());
        }
        if pflags.contains(OpenFlags::TRUNCATE) {
            files.insert(path.clone(), Vec::new());
        }
        drop(files);
        let handle = format!("h{}", self.next_handle.fetch_add(1, Ordering::SeqCst));
        let write = pflags.contains(OpenFlags::WRITE) || pflags.contains(OpenFlags::CREATE);
        self.handles
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?
            .insert(handle.clone(), SftpHandleState::File { path, write });
        Ok(Handle { id, handle })
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.handles
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?
            .remove(&handle);
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: "en-US".into(),
        })
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let state = self
            .handles
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?
            .get(&handle)
            .cloned();
        let SftpHandleState::File { path, .. } =
            state.ok_or_else(|| StatusReply::new(StatusCode::Failure))?
        else {
            return Err(StatusReply::new(StatusCode::Failure));
        };
        let files = self
            .files
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?;
        let content = files
            .get(&path)
            .ok_or_else(|| StatusReply::new(StatusCode::NoSuchFile))?;
        let start = offset.min(content.len() as u64) as usize;
        let end = (start as u64 + u64::from(len)).min(content.len() as u64) as usize;
        if start >= content.len() {
            return Err(StatusReply::new(StatusCode::Eof));
        }
        Ok(Data {
            id,
            data: content[start..end].to_vec(),
        })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let state = self
            .handles
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?
            .get(&handle)
            .cloned();
        let SftpHandleState::File { path, write } =
            state.ok_or_else(|| StatusReply::new(StatusCode::Failure))?
        else {
            return Err(StatusReply::new(StatusCode::Failure));
        };
        if !write {
            return Err(StatusReply::new(StatusCode::PermissionDenied));
        }
        let mut files = self
            .files
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?;
        let content = files.entry(path).or_insert_with(Vec::new);
        let start = offset as usize;
        let end = start.saturating_add(data.len());
        if content.len() < end {
            content.resize(end, 0);
        }
        content[start..end].copy_from_slice(&data);
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: "en-US".into(),
        })
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let attrs = self.attrs_for(&path)?;
        Ok(Attrs { id, attrs })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let attrs = self.attrs_for(&path)?;
        Ok(Attrs { id, attrs })
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let entries = self.list_dir(&path)?;
        let handle = format!("d{}", self.next_handle.fetch_add(1, Ordering::SeqCst));
        self.handles
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?
            .insert(
                handle.clone(),
                SftpHandleState::Dir {
                    entries,
                    exhausted: false,
                },
            );
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        let mut handles = self
            .handles
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?;
        let state = handles
            .get_mut(&handle)
            .ok_or_else(|| StatusReply::new(StatusCode::Failure))?;
        let SftpHandleState::Dir {
            entries, exhausted, ..
        } = state
        else {
            return Err(StatusReply::new(StatusCode::Failure));
        };
        if *exhausted {
            return Err(StatusReply::new(StatusCode::Eof));
        }
        *exhausted = true;
        Ok(Name {
            id,
            files: std::mem::take(entries),
        })
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        let path = normalize(&filename);
        let mut files = self
            .files
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?;
        if files.remove(&path).is_none() {
            return Err(StatusReply::new(StatusCode::NoSuchFile));
        }
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: "en-US".into(),
        })
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        let mut dirs = self
            .dirs
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?;
        if !dirs.insert(normalize(&path)) {
            return Err(StatusReply::new(StatusCode::Failure));
        }
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: "en-US".into(),
        })
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        let path = normalize(&path);
        let mut dirs = self
            .dirs
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?;
        if !dirs.remove(&path) {
            return Err(StatusReply::new(StatusCode::NoSuchFile));
        }
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: "en-US".into(),
        })
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        Ok(Name {
            id,
            files: vec![File::dummy(normalize(&path))],
        })
    }

    async fn extended(
        &mut self,
        id: u32,
        request: String,
        data: Vec<u8>,
    ) -> Result<Packet, Self::Error> {
        if request != extensions::EXPAND_PATH {
            return Err(StatusReply::new(StatusCode::OpUnsupported));
        }
        let mut bytes = bytes::Bytes::from(data);
        let request = de::from_bytes::<extensions::ExpandPathExtension>(&mut bytes)
            .map_err(|_| StatusReply::new(StatusCode::BadMessage))?;
        let path = match request.path.as_str() {
            "~" => "/home/tester".to_string(),
            path if path.starts_with("~/") => format!("/home/tester/{}", &path[2..]),
            path => normalize(path),
        };
        Ok(Packet::Name(Name {
            id,
            files: vec![File::dummy(path)],
        }))
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        let from = normalize(&oldpath);
        let to = normalize(&newpath);
        let mut files = self
            .files
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?;
        if let Some(content) = files.remove(&from) {
            files.insert(to, content);
            return Ok(Status {
                id,
                status_code: StatusCode::Ok,
                error_message: String::new(),
                language_tag: "en-US".into(),
            });
        }
        drop(files);
        let mut dirs = self
            .dirs
            .lock()
            .map_err(|_| StatusReply::new(StatusCode::Failure))?;
        if dirs.remove(&from) {
            dirs.insert(to);
            return Ok(Status {
                id,
                status_code: StatusCode::Ok,
                error_message: String::new(),
                language_tag: "en-US".into(),
            });
        }
        Err(StatusReply::new(StatusCode::NoSuchFile))
    }
}

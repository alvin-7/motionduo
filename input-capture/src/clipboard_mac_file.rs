//! Conservative descriptor-based macOS file URL reader.
use input_event::clipboard_png::MAX_PNG_BYTES;
use std::ffi::CString;
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

const SF_DATALESS: u32 = 0x4000_0000;

fn path_components(url: &[u8]) -> Result<Vec<CString>, ()> {
    let path = url.strip_prefix(b"file://").ok_or(())?;
    let slash = path.iter().position(|b| *b == b'/').ok_or(())?;
    let authority = &path[..slash];
    if !authority.is_empty() && !authority.eq_ignore_ascii_case(b"localhost") {
        return Err(());
    }
    let path = &path[slash + 1..];
    if path.contains(&b'?') || path.contains(&b'#') {
        return Err(());
    }
    let mut decoded = Vec::with_capacity(path.len());
    let mut i = 0;
    while i < path.len() {
        if path[i] == b'%' {
            let hex = |b: u8| (b as char).to_digit(16).map(|v| v as u8).ok_or(());
            let a = hex(*path.get(i + 1).ok_or(())?)?;
            let b = hex(*path.get(i + 2).ok_or(())?)?;
            let c = a * 16 + b;
            // Encoded separators are ambiguous for component-wise traversal.
            if c == b'/' {
                return Err(());
            }
            decoded.push(c);
            i += 3;
        } else {
            decoded.push(path[i]);
            i += 1;
        }
    }
    decoded
        .split(|b| *b == b'/')
        .map(|part| {
            if part.is_empty() || part == b"." || part == b".." {
                return Err(());
            }
            CString::new(part).map_err(|_| ())
        })
        .collect()
}

fn stat(fd: &OwnedFd) -> Result<libc::stat, ()> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(fd.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
        return Err(());
    }
    Ok(unsafe { stat.assume_init() })
}

fn check_local(fd: &OwnedFd) -> Result<(), ()> {
    let mut fs = std::mem::MaybeUninit::<libc::statfs>::uninit();
    if unsafe { libc::fstatfs(fd.as_raw_fd(), fs.as_mut_ptr()) } != 0 {
        return Err(());
    }
    if unsafe { fs.assume_init() }.f_flags & libc::MNT_LOCAL as u32 == 0 {
        return Err(());
    }
    if stat(fd)?.st_flags & SF_DATALESS != 0 {
        return Err(());
    }
    Ok(())
}

fn same_file(a: &libc::stat, b: &libc::stat) -> bool {
    a.st_dev == b.st_dev
        && a.st_ino == b.st_ino
        && a.st_size == b.st_size
        && a.st_mtime == b.st_mtime
        && a.st_mtime_nsec == b.st_mtime_nsec
        && a.st_ctime == b.st_ctime
        && a.st_ctime_nsec == b.st_ctime_nsec
        && a.st_flags == b.st_flags
        && a.st_mode == b.st_mode
}

fn read_limited(reader: impl Read) -> Result<Vec<u8>, ()> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_PNG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() > MAX_PNG_BYTES {
        return Err(());
    }
    Ok(bytes)
}

pub(super) fn read(url: &[u8]) -> Result<Vec<u8>, ()> {
    let components = path_components(url)?;
    let root = unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if root < 0 {
        return Err(());
    }
    let mut dir = unsafe { OwnedFd::from_raw_fd(root) };
    check_local(&dir)?;
    for (index, component) in components.iter().enumerate() {
        let last = index + 1 == components.len();
        if last {
            // Avoid opening an already-identifiable special file; the descriptor
            // check below still decides eligibility if the path changes here.
            let mut pre = std::mem::MaybeUninit::<libc::stat>::uninit();
            if unsafe {
                libc::fstatat(
                    dir.as_raw_fd(),
                    component.as_ptr(),
                    pre.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } != 0
            {
                return Err(());
            }
            let pre = unsafe { pre.assume_init() };
            if pre.st_mode & libc::S_IFMT != libc::S_IFREG || pre.st_flags & SF_DATALESS != 0 {
                return Err(());
            }
        }
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | libc::O_NONBLOCK
            | if last { 0 } else { libc::O_DIRECTORY };
        let fd = unsafe { libc::openat(dir.as_raw_fd(), component.as_ptr(), flags) };
        if fd < 0 {
            return Err(());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        check_local(&fd)?;
        if !last {
            dir = fd;
            continue;
        }
        let before = stat(&fd)?;
        if before.st_flags & SF_DATALESS != 0
            || before.st_mode & libc::S_IFMT != libc::S_IFREG
            || before.st_size < 0
            || before.st_size as u64 > MAX_PNG_BYTES as u64
        {
            return Err(());
        }
        let mut file = std::fs::File::from(fd);
        let bytes = read_limited(&mut file)?;
        let fd: OwnedFd = file.into();
        let after = stat(&fd)?;
        check_local(&fd)?;
        if !same_file(&before, &after) || bytes.len() as i64 != before.st_size {
            return Err(());
        }
        return Ok(bytes);
    }
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::{ffi::OsStrExt, fs::symlink};
    #[test]
    fn file_url_parsing_rejects_ambiguity() {
        assert!(path_components(b"file:///tmp/a%20b.png").is_ok());
        assert!(path_components(b"file://localhost/tmp/a.png").is_ok());
        for url in [
            b"file://remote/tmp/a".as_slice(),
            b"file:///tmp/../a",
            b"file:///tmp/%2e%2e/a",
            b"file:///tmp/a%2fb",
            b"file:///tmp/a?x",
            b"file:///tmp/%00",
            b"file:///tmp//a",
        ] {
            assert!(path_components(url).is_err());
        }
    }

    #[test]
    fn local_descriptor_fixtures_reject_symlinks_special_and_oversize() {
        let temp = tempfile::Builder::new()
            .prefix("mousehop-image-")
            .tempdir_in("/private/tmp")
            .unwrap();
        let base = temp.path();
        let url = |path: &std::path::Path| {
            let mut url = b"file://".to_vec();
            url.extend_from_slice(path.as_os_str().as_bytes());
            url
        };
        let dir = base.join("directory");
        std::fs::create_dir(&dir).unwrap();
        let regular = dir.join("image.png");
        std::fs::write(&regular, b"synthetic image bytes").unwrap();
        assert_eq!(read(&url(&regular)).unwrap(), b"synthetic image bytes");
        let final_link = base.join("final-link");
        symlink(&regular, &final_link).unwrap();
        assert!(read(&url(&final_link)).is_err());
        let dir_link = base.join("dir-link");
        symlink(&dir, &dir_link).unwrap();
        assert!(read(&url(&dir_link.join("image.png"))).is_err());
        assert!(read(&url(&dir)).is_err());
        let fifo = base.join("fifo");
        let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
        assert!(read(&url(&fifo)).is_err());
        let large = base.join("oversized");
        std::fs::File::create(&large)
            .unwrap()
            .set_len(MAX_PNG_BYTES as u64 + 1)
            .unwrap();
        assert!(read(&url(&large)).is_err());
    }

    #[test]
    fn read_limit_rejects_actual_extra_byte() {
        let mut reader = std::io::repeat(0).take(MAX_PNG_BYTES as u64 + 2);
        assert!(read_limited(&mut reader).is_err());
        let mut next = [0];
        assert_eq!(reader.read(&mut next).unwrap(), 1);
    }
}

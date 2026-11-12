//! Native owner-only storage. Checks and reads use the same opened object.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

fn refused() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "Chat storage must be an owned, private local directory with no links",
    )
}

/// Resolve through existing ancestors without hiding access errors or unresolved `..`.
pub fn resolve(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(refused());
    }
    let mut clean = PathBuf::new();
    for c in path.components() {
        match c {
            std::path::Component::ParentDir => {
                if !clean.pop() {
                    return Err(refused());
                }
            }
            std::path::Component::CurDir => {}
            _ => clean.push(c.as_os_str()),
        }
    }
    let mut ancestor = clean.as_path();
    let mut tail = Vec::new();
    loop {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                tail.push(ancestor.file_name().ok_or_else(refused)?.to_owned());
                ancestor = ancestor.parent().ok_or_else(refused)?;
            }
            Err(e) => return Err(e),
        }
    }
    let mut resolved = ancestor.canonicalize()?;
    for name in tail.into_iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}
pub fn contains(parent: &Path, child: &Path) -> bool {
    #[cfg(windows)]
    {
        let a = parent
            .components()
            .map(|s| s.as_os_str().to_string_lossy().to_lowercase())
            .collect::<Vec<_>>();
        let b = child
            .components()
            .map(|s| s.as_os_str().to_string_lossy().to_lowercase())
            .collect::<Vec<_>>();
        b.starts_with(&a)
    }
    #[cfg(not(windows))]
    {
        child.starts_with(parent)
    }
}
pub fn create_private_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            // Already private: nothing to change, and no right to change it is asked for.
            if verify_directory(path).is_ok() {
                return Ok(());
            }
            let f = platform::open(path, true, false)?;
            platform::validate(&f, true, false)?;
            platform::harden(&f, true)?;
            platform::validate(&f, true, true)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent()
                && !parent.exists()
            {
                create_private_directory(parent)?;
            }
            match platform::mkdir(path) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
            verify_directory(path)
        }
        Err(e) => Err(e),
    }
}
pub fn verify_directory(path: &Path) -> io::Result<()> {
    let f = platform::open(path, false, false)?;
    platform::validate(&f, true, true)
}
/// Keep a native directory handle alive to prevent replacement on Windows.
pub fn hold_directory(path: &Path) -> io::Result<File> {
    let f = platform::open(path, false, false)?;
    platform::validate(&f, true, true)?;
    Ok(f)
}
pub fn protect_file(path: &Path) -> io::Result<()> {
    let f = platform::open(path, true, false)?;
    platform::validate(&f, false, false)?;
    platform::harden(&f, false)?;
    platform::validate(&f, false, true)
}
pub fn verify_file(path: &Path) -> io::Result<()> {
    let f = platform::open(path, false, false)?;
    platform::validate(&f, false, true)
}
pub fn read_private(path: &Path) -> io::Result<Vec<u8>> {
    let f = platform::open(path, false, false)?;
    platform::validate(&f, false, true)?;
    let mut b = Vec::new();
    f.take(65537).read_to_end(&mut b)?;
    if b.len() > 65536 {
        return Err(io::Error::other("Chat operational file is too large"));
    }
    Ok(b)
}
pub fn open_lock(path: &Path, create: bool) -> io::Result<File> {
    let f = platform::open(path, true, create)?;
    platform::validate(&f, false, true)?;
    Ok(f)
}
/// New files are private at creation, never truncate an existing destination.
pub fn create_private_file(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let f = opts.open(path)?;
    // On Windows callers must first establish a protected parent with inheritable ACLs.
    platform::validate(&f, false, true)?;
    Ok(f)
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if path.exists() {
        verify_file(path)?;
    }
    let tmp = path.with_file_name(format!(".publish-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut f = create_private_file(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        platform::replace(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
/// Publish a completed export without replacing an existing path, even under a race.
pub fn publish_new(tmp: &Path, path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        platform::move_new(tmp, path)
    }
    #[cfg(unix)]
    {
        fs::hard_link(tmp, path)?;
        fs::remove_file(tmp)
    }
}
pub fn ensure_local(path: &Path) -> io::Result<()> {
    let mut p = path;
    while !p.exists() {
        p = p.parent().ok_or_else(refused)?;
    }
    platform::local(p)
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        ffi::{OsStr, c_void},
        mem::{size_of, zeroed},
        os::windows::{
            ffi::OsStrExt,
            fs::OpenOptionsExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
        },
        ptr::null_mut,
    };
    use windows_sys::Win32::{
        Foundation::{HANDLE, LocalFree},
        Security::{Authorization::*, *},
        Storage::FileSystem::*,
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    fn wide(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain([0]).collect()
    }
    fn win(v: i32) -> io::Result<()> {
        if v == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    struct Local(*mut c_void);
    impl Drop for Local {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    fn token() -> io::Result<Vec<usize>> {
        unsafe {
            let mut h: HANDLE = null_mut();
            win(OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut h))?;
            let _h = OwnedHandle::from_raw_handle(h);
            let mut n = 0;
            GetTokenInformation(h, TokenUser, null_mut(), 0, &mut n);
            let mut b = vec![0usize; (n as usize).div_ceil(size_of::<usize>())];
            win(GetTokenInformation(
                h,
                TokenUser,
                b.as_mut_ptr().cast(),
                n,
                &mut n,
            ))?;
            Ok(b)
        }
    }
    fn descriptor(dir: bool) -> io::Result<Local> {
        unsafe {
            let b = token()?;
            let sid = (*b.as_ptr().cast::<TOKEN_USER>()).User.Sid;
            let mut text = null_mut();
            win(ConvertSidToStringSidW(sid, &mut text))?;
            let _text = Local(text.cast());
            let n = (0..).take_while(|&i| *text.add(i) != 0).count();
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(text, n));
            let flags = if dir { "OICI" } else { "" };
            let sddl = wide(OsStr::new(&format!("O:{s}D:P(A;{flags};FA;;;{s})")));
            let mut d = null_mut();
            win(ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut d,
                null_mut(),
            ))?;
            Ok(Local(d))
        }
    }
    pub fn mkdir(path: &Path) -> io::Result<()> {
        unsafe {
            let d = descriptor(true)?;
            let a = SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: d.0,
                bInheritHandle: 0,
            };
            win(CreateDirectoryW(wide(path.as_os_str()).as_ptr(), &a))
        }
    }
    pub fn open(path: &Path, write: bool, create: bool) -> io::Result<File> {
        let mut o = OpenOptions::new();
        o.access_mode(
            READ_CONTROL
                | FILE_READ_ATTRIBUTES
                | GENERIC_READ_ACCESS
                | if write {
                    WRITE_DAC | GENERIC_WRITE_ACCESS
                } else {
                    0
                },
        )
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS);
        if create {
            o.write(true).create(true).truncate(false);
        }
        o.open(path)
    }
    const GENERIC_READ_ACCESS: u32 = 0x80000000;
    const GENERIC_WRITE_ACCESS: u32 = 0x40000000;
    pub fn validate(f: &File, dir: bool, private: bool) -> io::Result<()> {
        unsafe {
            let mut info: BY_HANDLE_FILE_INFORMATION = zeroed();
            win(GetFileInformationByHandle(f.as_raw_handle(), &mut info))?;
            if info.dwFileAttributes
                & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_OFFLINE | 0x00400000)
                != 0
                || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != dir
                || (!dir && info.nNumberOfLinks != 1)
            {
                return Err(refused());
            }
            let (mut owner, mut acl, mut desc) = (null_mut(), null_mut(), null_mut());
            let e = GetSecurityInfo(
                f.as_raw_handle(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                null_mut(),
                &mut acl,
                null_mut(),
                &mut desc,
            );
            if e != 0 {
                return Err(io::Error::from_raw_os_error(e as i32));
            }
            let _desc = Local(desc);
            let b = token()?;
            let sid = (*b.as_ptr().cast::<TOKEN_USER>()).User.Sid;
            if owner.is_null() || EqualSid(owner, sid) == 0 {
                return Err(refused());
            }
            if !private {
                return Ok(());
            }
            if acl.is_null() {
                return Err(refused());
            }
            let (mut control, mut revision) = (0, 0);
            win(GetSecurityDescriptorControl(
                desc,
                &mut control,
                &mut revision,
            ))?;
            if dir && control & SE_DACL_PROTECTED == 0 {
                return Err(refused());
            }
            let mut size: ACL_SIZE_INFORMATION = zeroed();
            win(GetAclInformation(
                acl,
                (&mut size as *mut ACL_SIZE_INFORMATION).cast(),
                size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            ))?;
            if size.AceCount != 1 {
                return Err(refused());
            }
            let mut ptr = null_mut();
            win(GetAce(acl, 0, &mut ptr))?;
            let ace = &*ptr.cast::<ACCESS_ALLOWED_ACE>();
            if ace.Header.AceType != 0
                || ace.Header.AceFlags & 8 != 0
                || EqualSid((&raw const ace.SidStart).cast_mut().cast(), sid) == 0
                || ace.Mask != FILE_ALL_ACCESS
                || (dir && ace.Header.AceFlags & 3 != 3)
            {
                return Err(refused());
            }
            Ok(())
        }
    }
    pub fn harden(f: &File, dir: bool) -> io::Result<()> {
        unsafe {
            let d = descriptor(dir)?;
            let (mut present, mut defaulted, mut acl) = (0, 0, null_mut());
            win(GetSecurityDescriptorDacl(
                d.0,
                &mut present,
                &mut acl,
                &mut defaulted,
            ))?;
            let e = SetSecurityInfo(
                f.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null_mut(),
            );
            if e == 0 {
                Ok(())
            } else {
                Err(io::Error::from_raw_os_error(e as i32))
            }
        }
    }
    pub fn replace(a: &Path, b: &Path) -> io::Result<()> {
        unsafe {
            win(MoveFileExW(
                wide(a.as_os_str()).as_ptr(),
                wide(b.as_os_str()).as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            ))
        }
    }
    pub fn move_new(a: &Path, b: &Path) -> io::Result<()> {
        unsafe {
            win(MoveFileExW(
                wide(a.as_os_str()).as_ptr(),
                wide(b.as_os_str()).as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            ))
        }
    }
    pub fn local(path: &Path) -> io::Result<()> {
        unsafe {
            let mut volume = vec![0u16; 32768];
            win(GetVolumePathNameW(
                wide(path.as_os_str()).as_ptr(),
                volume.as_mut_ptr(),
                volume.len() as u32,
            ))?;
            let kind = GetDriveTypeW(volume.as_ptr());
            if !matches!(kind, 2 | 3) {
                return Err(refused());
            }
            let mut flags = 0;
            win(GetVolumeInformationW(
                volume.as_ptr(),
                null_mut(),
                0,
                null_mut(),
                null_mut(),
                &mut flags,
                null_mut(),
                0,
            ))?;
            if flags & 0x8 == 0 {
                return Err(refused());
            }
            Ok(())
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    };
    pub fn mkdir(p: &Path) -> io::Result<()> {
        fs::DirBuilder::new().mode(0o700).create(p)
    }
    pub fn open(p: &Path, write: bool, create: bool) -> io::Result<File> {
        let mut o = OpenOptions::new();
        o.read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        if write && !p.is_dir() {
            o.write(true);
        }
        if create {
            o.create(true).mode(0o600).truncate(false);
        }
        o.open(p)
    }
    pub fn validate(f: &File, dir: bool, private: bool) -> io::Result<()> {
        let m = f.metadata()?;
        if m.uid() != unsafe { libc::geteuid() }
            || m.is_dir() != dir
            || (!dir && (!m.is_file() || m.nlink() != 1))
            || (private && m.mode() & 0o077 != 0)
        {
            return Err(refused());
        }
        Ok(())
    }
    pub fn harden(f: &File, dir: bool) -> io::Result<()> {
        f.set_permissions(fs::Permissions::from_mode(if dir { 0o700 } else { 0o600 }))
    }
    pub fn replace(a: &Path, b: &Path) -> io::Result<()> {
        fs::rename(a, b)?;
        File::open(b.parent().ok_or_else(refused)?)?.sync_all()
    }
    pub fn local(p: &Path) -> io::Result<()> {
        let p = std::ffi::CString::new(p.as_os_str().as_bytes()).map_err(|_| refused())?;
        #[cfg(target_os = "linux")]
        unsafe {
            let mut s = std::mem::zeroed();
            if libc::statfs(p.as_ptr(), &mut s) != 0 {
                return Err(io::Error::last_os_error());
            }
            if !matches!(
                s.f_type as u64,
                0xef53
                    | 0x58465342
                    | 0x9123683e
                    | 0x2fc12fc1
                    | 0x01021994
                    | 0x794c7630
                    | 0x858458f6
            ) {
                return Err(refused());
            }
            Ok(())
        }
        #[cfg(target_os = "macos")]
        unsafe {
            let mut s = std::mem::zeroed();
            if libc::statfs(p.as_ptr(), &mut s) != 0 {
                return Err(io::Error::last_os_error());
            }
            if s.f_flags & libc::MNT_LOCAL == 0 {
                return Err(refused());
            }
            Ok(())
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = p;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Local filesystem verification is unsupported on this platform",
            ))
        }
    }
}

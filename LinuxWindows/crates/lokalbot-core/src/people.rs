//! People: profiles joined from calendar attendees, speaker names and action
//! owners (`PeopleDirectory.swift`, `PersonNotes.swift`).

use std::sync::OnceLock;

/// `person-%016llx` of a join key (`email:<address>` or `name:<normalized>`).
pub fn person_id(key: &str) -> String {
    format!("person-{}", crate::ids::fnv_hex(key))
}

/// The ID a name-only person gets.
pub fn person_id_for_name(name: &str) -> String {
    person_id(&format!("name:{}", crate::ids::fold(name)))
}

/// The user's own name: the configured autocomplete name when set, else
/// the operating-system account's full name (`NSFullUserName` on macOS).
pub fn user_full_name(configured: Option<&str>) -> Option<String> {
    if let Some(name) = configured.map(str::trim).filter(|n| !n.is_empty()) {
        return Some(name.to_string());
    }
    static OS_NAME: OnceLock<Option<String>> = OnceLock::new();
    OS_NAME.get_or_init(os_full_name).clone()
}

#[cfg(unix)]
fn os_full_name() -> Option<String> {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .ok()?;
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd_full_name(&passwd, &user)
}

#[cfg(windows)]
fn os_full_name() -> Option<String> {
    windows_full_name()
}

#[cfg(not(any(unix, windows)))]
fn os_full_name() -> Option<String> {
    None
}

/// The GECOS full name (before the first comma) of `user` in `/etc/passwd`.
pub fn passwd_full_name(passwd: &str, user: &str) -> Option<String> {
    passwd.lines().find_map(|line| {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() < 5 || fields[0] != user {
            return None;
        }
        let name = fields[4].split(',').next().unwrap_or("").trim();
        (!name.is_empty()).then(|| name.to_string())
    })
}

#[cfg(windows)]
fn windows_full_name() -> Option<String> {
    use windows_sys::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};
    let mut size: u32 = 0;
    // SAFETY: a null buffer with size 0 only queries the required length.
    unsafe { GetUserNameExW(NameDisplay, std::ptr::null_mut(), &mut size) };
    if size == 0 {
        return None;
    }
    let mut buffer = vec![0u16; size as usize + 1];
    // SAFETY: `buffer` holds `size` UTF-16 units plus a terminator, as requested.
    let ok = unsafe { GetUserNameExW(NameDisplay, buffer.as_mut_ptr(), &mut size) };
    if ok == 0 {
        return None;
    }
    let name = String::from_utf16_lossy(&buffer[..size as usize])
        .trim()
        .to_string();
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gecos_full_name() {
        let passwd =
            "root:x:0:0:root:/root:/bin/bash\nmaya:x:1000:1000:Maya Chen,,,:/home/maya:/bin/bash\n";
        assert_eq!(
            passwd_full_name(passwd, "maya").as_deref(),
            Some("Maya Chen")
        );
        assert_eq!(passwd_full_name(passwd, "nobody"), None);
        assert_eq!(user_full_name(Some(" Stevan ")).as_deref(), Some("Stevan"));
    }
}

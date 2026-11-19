//! The password check, as vlock-main makes it (vlock 2.2.2 src/auth-pam.c): `pam_start` on the
//! `vlock` service for the writer, `PAM_TTY`, `pam_authenticate`, `pam_end`, and nothing else.
//! Run as the writer, pam_unix checks the password through its setuid `unix_chkpwd` helper,
//! which only ever checks the calling user's own password.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;

const PAM_SUCCESS: c_int = 0;
const PAM_BUF_ERR: c_int = 5;
const PAM_CONV_ERR: c_int = 19;
const PAM_TTY: c_int = 3;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;

#[repr(C)]
struct Message {
    style: c_int,
    text: *const c_char,
}

#[repr(C)]
struct Response {
    text: *mut c_char,
    code: c_int,
}

type Converse = extern "C" fn(c_int, *mut *const Message, *mut *mut Response, *mut c_void) -> c_int;

#[repr(C)]
struct Conversation {
    converse: Option<Converse>,
    data: *mut c_void,
}

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(
        service: *const c_char,
        user: *const c_char,
        conversation: *const Conversation,
        handle: *mut *mut c_void,
    ) -> c_int;
    fn pam_set_item(handle: *mut c_void, item: c_int, value: *const c_void) -> c_int;
    fn pam_authenticate(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_end(handle: *mut c_void, status: c_int) -> c_int;
}

/// What the conversation answers with: the password for a hidden question, the account name
/// for a visible one (pam_unix asks neither when the account is set, but a module may).
struct Answers<'a> {
    user: &'a CStr,
    password: &'a [u8],
}

/// A NUL-terminated copy in memory PAM owns: Linux-PAM wipes and frees each answer
/// (`_pam_drop_reply`).
unsafe fn answer(text: &[u8]) -> *mut c_char {
    let copy = unsafe { libc::malloc(text.len() + 1) } as *mut u8;
    if !copy.is_null() {
        unsafe {
            ptr::copy_nonoverlapping(text.as_ptr(), copy, text.len());
            *copy.add(text.len()) = 0;
        }
    }
    copy as *mut c_char
}

extern "C" fn converse(
    count: c_int,
    messages: *mut *const Message,
    responses: *mut *mut Response,
    data: *mut c_void,
) -> c_int {
    if count <= 0 || count > 32 || messages.is_null() || responses.is_null() || data.is_null() {
        return PAM_CONV_ERR;
    }
    let answers = unsafe { &*(data as *const Answers) };
    let count = count as usize;
    let replies = unsafe { libc::calloc(count, std::mem::size_of::<Response>()) } as *mut Response;
    if replies.is_null() {
        return PAM_BUF_ERR;
    }
    for index in 0..count {
        // Linux-PAM passes an array of pointers to messages.
        let message = unsafe { *messages.add(index) };
        if message.is_null() {
            continue;
        }
        let text = match unsafe { (*message).style } {
            PAM_PROMPT_ECHO_OFF => unsafe { answer(answers.password) },
            PAM_PROMPT_ECHO_ON => unsafe { answer(answers.user.to_bytes()) },
            // Notices and errors (pam_faildelay, a lockout's message) need no answer.
            _ => continue,
        };
        if text.is_null() {
            for done in 0..index {
                let earlier = unsafe { (*replies.add(done)).text };
                if !earlier.is_null() {
                    unsafe {
                        ptr::write_bytes(earlier, 0, libc::strlen(earlier));
                        libc::free(earlier as *mut c_void);
                    }
                }
            }
            unsafe { libc::free(replies as *mut c_void) };
            return PAM_BUF_ERR;
        }
        unsafe { (*replies.add(index)).text = text };
    }
    unsafe { *responses = replies };
    PAM_SUCCESS
}

/// True only when PAM's `vlock` stack accepts `password` for `user`.
pub fn authenticate(user: &CStr, password: &str, tty: &CStr) -> bool {
    // A NUL would cut the password short in C; no real password contains one.
    if password.as_bytes().contains(&0) {
        return false;
    }
    let answers = Answers {
        user,
        password: password.as_bytes(),
    };
    let conversation = Conversation {
        converse: Some(converse),
        data: &answers as *const Answers as *mut c_void,
    };
    let service = CString::from(c"vlock");
    let mut handle = ptr::null_mut();
    let started = unsafe { pam_start(service.as_ptr(), user.as_ptr(), &conversation, &mut handle) };
    if started != PAM_SUCCESS {
        if !handle.is_null() {
            unsafe { pam_end(handle, started) };
        }
        return false;
    }
    let mut status = unsafe { pam_set_item(handle, PAM_TTY, tty.as_ptr() as *const c_void) };
    if status == PAM_SUCCESS {
        status = unsafe { pam_authenticate(handle, 0) };
    }
    unsafe { pam_end(handle, status) };
    status == PAM_SUCCESS
}

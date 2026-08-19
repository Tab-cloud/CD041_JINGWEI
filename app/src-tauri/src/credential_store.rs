#[cfg(windows)]
const CREDENTIAL_TARGET: &str = "Jingwei Agent: jingwei.openai";

#[cfg(windows)]
mod platform {
    use super::CREDENTIAL_TARGET;
    use std::ptr;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };
    use zeroize::Zeroize;

    struct CredentialBuffer(*mut CREDENTIALW);

    impl Drop for CredentialBuffer {
        fn drop(&mut self) {
            if self.0.is_null() {
                return;
            }
            unsafe {
                let credential = &mut *self.0;
                if !credential.CredentialBlob.is_null() && credential.CredentialBlobSize > 0 {
                    let secret = std::slice::from_raw_parts_mut(
                        credential.CredentialBlob,
                        credential.CredentialBlobSize as usize,
                    );
                    secret.zeroize();
                }
                CredFree(self.0.cast());
            }
        }
    }

    pub(crate) fn write(secret: &str) -> Result<(), String> {
        write_for_target(CREDENTIAL_TARGET, secret)
    }

    fn write_for_target(target_name: &str, secret: &str) -> Result<(), String> {
        let mut target = wide_string(target_name);
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            CredentialBlobSize: secret.len() as u32,
            CredentialBlob: secret.as_ptr().cast_mut(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..CREDENTIALW::default()
        };
        let written = unsafe { CredWriteW(&credential, 0) };
        target.zeroize();
        if written == 0 {
            return Err("Windows Credential Manager 拒绝写入；API Key 未保存".into());
        }
        Ok(())
    }

    pub(crate) fn read() -> Result<Option<String>, String> {
        read_for_target(CREDENTIAL_TARGET)
    }

    fn read_for_target(target_name: &str) -> Result<Option<String>, String> {
        let mut target = wide_string(target_name);
        let mut raw = ptr::null_mut();
        let found = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) };
        target.zeroize();
        if found == 0 {
            let error = unsafe { GetLastError() };
            if error == ERROR_NOT_FOUND {
                return Ok(None);
            }
            return Err("无法读取 Windows Credential Manager 状态；API Key 未返回前端".into());
        }

        let buffer = CredentialBuffer(raw);
        let credential = unsafe { &*buffer.0 };
        if credential.CredentialBlob.is_null() || credential.CredentialBlobSize == 0 {
            return Ok(None);
        }
        let bytes = unsafe {
            std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
        };
        String::from_utf8(bytes.to_vec())
            .map(Some)
            .map_err(|_| "系统凭据内容格式无效".into())
    }

    pub(crate) fn delete() -> Result<(), String> {
        delete_for_target(CREDENTIAL_TARGET)
    }

    fn delete_for_target(target_name: &str) -> Result<(), String> {
        let mut target = wide_string(target_name);
        let deleted = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
        target.zeroize();
        if deleted == 0 {
            let error = unsafe { GetLastError() };
            if error != ERROR_NOT_FOUND {
                return Err("无法从 Windows Credential Manager 删除 API Key".into());
            }
        }
        Ok(())
    }

    fn wide_string(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use zeroize::Zeroize;

        struct Cleanup(String);

        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = delete_for_target(&self.0);
            }
        }

        #[test]
        fn round_trips_an_isolated_windows_credential() {
            let target = format!("Jingwei Agent test: {}", uuid::Uuid::new_v4());
            let _cleanup = Cleanup(target.clone());
            write_for_target(&target, "unit-test-secret").unwrap();
            let mut stored = read_for_target(&target).unwrap().unwrap();
            assert_eq!(stored, "unit-test-secret");
            stored.zeroize();
            delete_for_target(&target).unwrap();
            assert!(read_for_target(&target).unwrap().is_none());
        }
    }
}

#[cfg(not(windows))]
mod platform {
    fn unavailable<T>() -> Result<T, String> {
        Err(
            "当前构建尚未提供 macOS Keychain 或 Linux Secret Service 适配器；不会回退到明文文件"
                .into(),
        )
    }

    pub(crate) fn write(_secret: &str) -> Result<(), String> {
        unavailable()
    }

    pub(crate) fn read() -> Result<Option<String>, String> {
        unavailable()
    }

    pub(crate) fn delete() -> Result<(), String> {
        unavailable()
    }
}

pub(crate) use platform::{delete, read, write};

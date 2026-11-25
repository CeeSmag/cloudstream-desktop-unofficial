//! Reading a file's Authenticode signature with Windows PowerShell's `Get-AuthenticodeSignature`
//! (WinVerifyTrust underneath), which needs no unsafe code here. The file's path is handed over in
//! an environment variable, never pasted into the command, so no file name can change the script.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{SetupError, SignatureVerifier};

/// `CREATE_NO_WINDOW`: no console flashes up.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const PATH_VARIABLE: &str = "CS_WEBVIEW2_SETUP";
/// Prints `Status|Subject` (an empty subject when the file is not signed).
const SCRIPT: &str = "$s = Get-AuthenticodeSignature -LiteralPath $env:CS_WEBVIEW2_SETUP; \
$n = ''; if ($s.SignerCertificate) { $n = $s.SignerCertificate.Subject }; \
[Console]::Out.Write(\"$($s.Status)|$n\")";

/// Reads signatures through PowerShell.
pub(crate) struct PowerShellSignature;

/// Windows PowerShell from the system folder (not whatever `powershell` is first on PATH).
fn powershell() -> PathBuf {
    let root =
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    root.join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
}

impl SignatureVerifier for PowerShellSignature {
    fn describe(&self, file: &Path) -> Result<String, SetupError> {
        let output = Command::new(powershell())
            .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
            .env(PATH_VARIABLE, file)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|error| SetupError::disk("start the signature check for", file, &error))?;
        if !output.status.success() {
            return Err(SetupError::NotMicrosoft(
                "the signature could not be read".into(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::judge_signature;

    /// T4: a Windows program (catalog-signed by Microsoft) passes; a plain text file does not.
    #[test]
    #[ignore = "runs Windows PowerShell"]
    fn t4_windows_files_are_judged_by_their_real_signature() {
        let notepad = powershell().parent().unwrap().join(r"..\..\notepad.exe");
        let answer = PowerShellSignature.describe(&notepad).unwrap();
        assert_eq!(judge_signature(&answer), Ok(()), "{answer}");

        let text = std::env::temp_dir().join(format!("cs_wv2_unsigned_{}.txt", std::process::id()));
        std::fs::write(&text, b"not signed").unwrap();
        let answer = PowerShellSignature.describe(&text).unwrap();
        assert!(judge_signature(&answer).is_err(), "{answer}");
        let _ = std::fs::remove_file(&text);
    }

    #[test]
    fn the_file_is_passed_by_variable_not_pasted_into_the_script() {
        assert!(SCRIPT.contains("$env:CS_WEBVIEW2_SETUP"));
        assert_eq!(PATH_VARIABLE, "CS_WEBVIEW2_SETUP");
        assert!(powershell().ends_with(r"WindowsPowerShell\v1.0\powershell.exe"));
    }
}

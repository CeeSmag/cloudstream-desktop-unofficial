//! Judging the signature of the downloaded setup: it must be a valid Authenticode signature by
//! Microsoft. The reading is done by a [`crate::SignatureVerifier`]; it answers `Status|Subject`.

use crate::SetupError;

/// The organisation field Microsoft's release certificates carry.
const MICROSOFT_ORGANISATION: &str = "O=Microsoft Corporation";

/// Accepts the verifier's answer only for a valid signature whose signer subject names Microsoft as
/// the organisation (as a whole field, so `O=Microsoft Corporation Ltd` is refused).
pub fn judge_signature(answer: &str) -> Result<(), SetupError> {
    let (status, subject) = answer
        .trim()
        .split_once('|')
        .ok_or_else(|| SetupError::NotMicrosoft("the signature could not be read".into()))?;
    if status.trim() != "Valid" {
        return Err(SetupError::NotMicrosoft(format!(
            "signature status {}",
            status.trim()
        )));
    }
    if subject
        .split(',')
        .any(|field| field.trim() == MICROSOFT_ORGANISATION)
    {
        Ok(())
    } else {
        Err(SetupError::NotMicrosoft("signed by someone else".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MICROSOFT: &str =
        "CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US";

    #[test]
    fn a_valid_microsoft_signature_passes() {
        assert_eq!(judge_signature(&format!("Valid|{MICROSOFT}\r\n")), Ok(()));
        assert_eq!(judge_signature("Valid|O=Microsoft Corporation"), Ok(()));
    }

    #[test]
    fn a_broken_or_missing_signature_is_refused() {
        for status in ["NotSigned", "HashMismatch", "UnknownError", "valid", ""] {
            let error = judge_signature(&format!("{status}|{MICROSOFT}")).unwrap_err();
            assert!(matches!(error, SetupError::NotMicrosoft(_)), "{status}");
        }
    }

    #[test]
    fn another_signer_is_refused() {
        for subject in [
            "CN=Microsoft Corporation, O=Evil Corp",
            "O=Microsoft Corporation Ltd, C=US",
            "CN=O=Microsoft Corporation",
            "",
        ] {
            assert_eq!(
                judge_signature(&format!("Valid|{subject}")),
                Err(SetupError::NotMicrosoft("signed by someone else".into())),
                "{subject}"
            );
        }
    }

    #[test]
    fn an_unreadable_answer_is_refused() {
        assert_eq!(
            judge_signature("Valid"),
            Err(SetupError::NotMicrosoft(
                "the signature could not be read".into()
            ))
        );
    }

    #[test]
    fn the_status_is_named_in_the_refusal() {
        assert_eq!(
            judge_signature(" NotSigned |"),
            Err(SetupError::NotMicrosoft(
                "signature status NotSigned".into()
            ))
        );
    }
}

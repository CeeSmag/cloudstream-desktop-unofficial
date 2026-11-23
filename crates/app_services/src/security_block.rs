//! Human-readable classification of runtime plugin security refusals.
//!
//! Translates raw JVM verifier / classloader findings into concise risk categories for the UI,
//! keeping technical details available on demand without dumping stack-like strings into toasts.

/// High-level risk category explaining why a plugin was blocked by the security sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecurityConcern {
    /// Spawning OS processes or invoking privileged `Runtime` / `System` controls.
    ProcessOrSystemControl,
    /// Loading arbitrary bytecode (`ClassLoader`, `MethodHandles`, `Unsafe`).
    DynamicCodeLoading,
    /// Enumerating local network interfaces or opening raw LAN/socket connections.
    LocalNetworkOrRawSockets,
    /// Accessing raw file descriptors or host filesystem APIs outside the plugin sandbox.
    FileSystemAccess,
    /// Using dynamic proxies or reflective access to restricted system types.
    RestrictedReflection,
    /// Disabling TLS certificate/hostname checks or bypassing SSRF-guarded HTTP routing.
    UnsafeNetworkOrTlsConfig,
    /// Referencing another restricted JDK or host-application class.
    RestrictedSystemApi,
}

impl SecurityConcern {
    /// Short label suitable for a single-line summary.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ProcessOrSystemControl => "system command or process control",
            Self::DynamicCodeLoading => "dynamic code or class loading",
            Self::LocalNetworkOrRawSockets => "local network or raw socket access",
            Self::FileSystemAccess => "direct file system access",
            Self::RestrictedReflection => "restricted reflection",
            Self::UnsafeNetworkOrTlsConfig => "unsafe network or TLS settings",
            Self::RestrictedSystemApi => "restricted system APIs",
        }
    }

    /// Plain-language bullet point for the security warning dialog.
    #[must_use]
    pub const fn explanation(self) -> &'static str {
        match self {
            Self::ProcessOrSystemControl => {
                "System commands & process control — can run OS commands or read environment secrets."
            }
            Self::DynamicCodeLoading => {
                "Dynamic class loading — can load unverified code and bypass runtime checks."
            }
            Self::LocalNetworkOrRawSockets => {
                "Local network & raw sockets — can scan your local network or open direct connections."
            }
            Self::FileSystemAccess => {
                "Direct file access — references low-level file system APIs outside the plugin folder."
            }
            Self::RestrictedReflection => {
                "Restricted reflection — uses reflective access to reach protected system internals."
            }
            Self::UnsafeNetworkOrTlsConfig => {
                "Custom network or TLS settings — can disable certificate checks or bypass HTTP guards."
            }
            Self::RestrictedSystemApi => {
                "Restricted system APIs — references protected platform or host internals."
            }
        }
    }
}

/// Structured details of a plugin refused by the security sandbox or hard classloader policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityBlock {
    /// Display name of the plugin shown to the user.
    pub plugin: String,
    /// File name of the plugin (`StreamCenter.cs3`).
    pub file_name: String,
    /// True when the user can bypass this refusal by explicitly trusting the plugin.
    pub can_trust: bool,
    /// Deduplicated, severity-ordered plain-language risk categories.
    pub concerns: Vec<SecurityConcern>,
    /// Raw violation strings reported by the runtime (shown only if expanded).
    pub technical_details: Vec<String>,
}

/// Raw security refusal payload forwarded from the extension runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityRefusal {
    /// True when the refusal came from the untrusted sandbox verifier rather than a hard deny.
    pub bypassable: bool,
    /// Distinct violation strings reported by the runtime.
    pub violations: Vec<String>,
    /// Fallback summary string when `violations` is empty.
    pub message: String,
}

impl SecurityBlock {
    /// Builds a [`SecurityBlock`] from a runtime [`SecurityRefusal`].
    ///
    /// `trusted` is authoritative: a plugin already loaded with `trusted = true` is never marked
    /// `can_trust = true`, preventing an endless trust retry loop on hard `EXPLICIT_DENY` blocks.
    #[must_use]
    pub fn from_refusal(
        plugin: String,
        file_name: String,
        trusted: bool,
        refusal: SecurityRefusal,
    ) -> Self {
        let technical_details = normalize_details(refusal.violations, &refusal.message);
        let concerns = classify_violations(&technical_details);
        Self {
            plugin,
            file_name,
            can_trust: !trusted && refusal.bypassable,
            concerns,
            technical_details,
        }
    }

    /// Short comma-separated list of risk labels for single-line errors and logs.
    #[must_use]
    pub fn short_summary(&self) -> String {
        let labels: Vec<&str> = self.concerns.iter().map(|c| c.label()).collect();
        if labels.is_empty() {
            SecurityConcern::RestrictedSystemApi.label().to_owned()
        } else {
            labels.join(", ")
        }
    }

    /// Plain-language bullet list for the security dialog.
    #[must_use]
    pub fn explanations(&self) -> Vec<String> {
        self.concerns
            .iter()
            .map(|concern| concern.explanation().to_owned())
            .collect()
    }
}

fn normalize_details(violations: Vec<String>, fallback_message: &str) -> Vec<String> {
    let cleaned: Vec<String> = violations
        .into_iter()
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
        .collect();
    if !cleaned.is_empty() {
        return cleaned;
    }
    let trimmed = fallback_message.trim();
    if trimmed.is_empty() {
        Vec::new()
    } else {
        trimmed
            .split("; ")
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect()
    }
}

/// Classifies raw violation strings into unique, severity-ordered [`SecurityConcern`] categories.
#[must_use]
pub fn classify_violations(violations: &[String]) -> Vec<SecurityConcern> {
    let mut found = Vec::new();
    for entry in violations {
        let concern = classify_one(entry);
        if !found.contains(&concern) {
            found.push(concern);
        }
    }
    if found.is_empty() {
        found.push(SecurityConcern::RestrictedSystemApi);
    }
    found.sort();
    found
}

const CONCERN_RULES: &[(&[&str], SecurityConcern)] = &[
    (
        &[
            "processbuilder",
            "java.lang.process",
            "runtime.exec",
            "runtime.load",
            "runtime.exit",
            "runtime.halt",
            "system.exit",
            "system.getenv",
            "system.getpropert",
            "system.load",
            "setsecuritymanager",
            "java.awt.desktop",
            "java.awt.robot",
        ],
        SecurityConcern::ProcessOrSystemControl,
    ),
    (
        &[
            "classloader",
            "methodhandle",
            "varhandle",
            "unsafe",
            "javax.script",
        ],
        SecurityConcern::DynamicCodeLoading,
    ),
    (
        &[
            "networkinterface",
            "interfaceaddress",
            "inetsocketaddress",
            "socketaddress",
            "datagrampacket",
            "datagramsocket",
            "serversocket",
            "java.net.socket",
            "raw socket",
            "isreachable",
            "getlocalhost",
            "java.net.proxy",
        ],
        SecurityConcern::LocalNetworkOrRawSockets,
    ),
    (
        &["filedescriptor", "java.nio.file", "randomaccessfile"],
        SecurityConcern::FileSystemAccess,
    ),
    (
        &[
            "reflect.proxy",
            "reflective",
            "class.forname",
            "java.lang.reflect",
        ],
        SecurityConcern::RestrictedReflection,
    ),
    (
        &[
            "sethostnameverifier",
            "setsslsocketfactory",
            "setdefaultsslsocketfactory",
            "setdefaulthostnameverifier",
            "setfollowredirects",
            "url.openconnection",
            "url.openstream",
            "url.getcontent",
        ],
        SecurityConcern::UnsafeNetworkOrTlsConfig,
    ),
];

fn classify_one(entry: &str) -> SecurityConcern {
    let lower = entry.to_ascii_lowercase();
    CONCERN_RULES
        .iter()
        .find(|(needles, _)| needles.iter().any(|needle| lower.contains(needle)))
        .map_or(SecurityConcern::RestrictedSystemApi, |(_, concern)| {
            *concern
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_streamcenter_violations_into_deduplicated_ordered_concerns() {
        let violations = vec![
            "Disallowed class 'java.io.FileDescriptor' referenced in a.b()".into(),
            "Disallowed class 'java.lang.ClassLoader' referenced in c.d()".into(),
            "Disallowed class 'java.net.NetworkInterface' referenced in e.f()".into(),
            "Disallowed class 'java.net.DatagramPacket' referenced in g.h()".into(),
            "Disallowed class 'java.lang.reflect.Proxy' referenced in i.j()".into(),
        ];
        let block = SecurityBlock::from_refusal(
            "StreamCenter".into(),
            "StreamCenter.cs3".into(),
            false,
            SecurityRefusal {
                bypassable: true,
                violations,
                message: String::new(),
            },
        );
        assert!(block.can_trust);
        assert_eq!(
            block.concerns,
            vec![
                SecurityConcern::DynamicCodeLoading,
                SecurityConcern::LocalNetworkOrRawSockets,
                SecurityConcern::FileSystemAccess,
                SecurityConcern::RestrictedReflection,
            ]
        );
        let explanations = block.explanations();
        assert_eq!(explanations.len(), 4);
        assert!(explanations[0].contains("Dynamic class loading"));
        assert!(explanations[1].contains("Local network"));
        assert!(explanations[2].contains("Direct file access"));
        assert!(explanations[3].contains("Restricted reflection"));
        assert!(block
            .short_summary()
            .contains("local network or raw socket access"));
    }

    #[test]
    fn every_security_concern_has_distinct_human_explanation() {
        let all = [
            SecurityConcern::ProcessOrSystemControl,
            SecurityConcern::DynamicCodeLoading,
            SecurityConcern::LocalNetworkOrRawSockets,
            SecurityConcern::FileSystemAccess,
            SecurityConcern::RestrictedReflection,
            SecurityConcern::UnsafeNetworkOrTlsConfig,
            SecurityConcern::RestrictedSystemApi,
        ];
        let texts: Vec<&str> = all.iter().map(|c| c.explanation()).collect();
        for text in &texts {
            assert!(
                text.contains('—'),
                "expected plain-English explanation: {text}"
            );
        }
        let unique: std::collections::BTreeSet<&str> = texts.iter().copied().collect();
        assert_eq!(unique.len(), all.len());
    }

    #[test]
    fn already_trusted_plugin_is_never_marked_can_trust() {
        let block = SecurityBlock::from_refusal(
            "HardBlocked".into(),
            "HardBlocked.cs3".into(),
            true,
            SecurityRefusal {
                bypassable: true,
                violations: vec!["Disallowed class 'java.lang.ProcessBuilder'".into()],
                message: String::new(),
            },
        );
        assert!(!block.can_trust);
        assert_eq!(
            block.concerns,
            vec![SecurityConcern::ProcessOrSystemControl]
        );
    }

    #[test]
    fn falls_back_to_splitting_raw_message_when_violations_empty() {
        let block = SecurityBlock::from_refusal(
            "OldSidecar".into(),
            "OldSidecar.cs3".into(),
            false,
            SecurityRefusal {
                bypassable: true,
                violations: Vec::new(),
                message: "Illegal URL.openConnection() call; Disallowed class 'com.unknown.Host'"
                    .into(),
            },
        );
        assert_eq!(block.technical_details.len(), 2);
        assert_eq!(
            block.concerns,
            vec![
                SecurityConcern::UnsafeNetworkOrTlsConfig,
                SecurityConcern::RestrictedSystemApi,
            ]
        );
    }

    #[test]
    fn empty_violations_and_message_still_yield_fallback_concern() {
        let block = SecurityBlock::from_refusal(
            "Mystery".into(),
            "Mystery.cs3".into(),
            false,
            SecurityRefusal {
                bypassable: false,
                violations: Vec::new(),
                message: "   ".into(),
            },
        );
        assert!(!block.can_trust);
        assert_eq!(block.concerns, vec![SecurityConcern::RestrictedSystemApi]);
        assert_eq!(block.short_summary(), "restricted system APIs");
    }
}

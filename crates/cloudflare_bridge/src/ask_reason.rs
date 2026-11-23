//! Why the sidecar asks for a check, from the optional lines of its request file.
//!
//! - `rejected=<pass id>`: the saved pass the app just handed out was refused by the site (its IP or browser
//!   identity no longer matches, or the site raised its protection). That pass is retired and the request
//!   goes to the check window: the same pass is never handed out in a loop.
//! - `need=browser`: the sidecar reaches this origin only through the browser window (its own HTTP client is
//!   refused even with cookies), so a cookie pass would not help it.
//!
//! Both are optional: a request without them is an ordinary challenge.

/// The saved pass a request reports as refused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RejectedPass {
    /// None: an ordinary challenge.
    #[default]
    None,
    /// This saved pass was refused.
    Named(crate::PassId),
    /// Something was refused but the id cannot be read: no pass is handed out, none is retired.
    Unnamed,
}

/// What the sidecar can use as an answer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RequestNeed {
    /// Cookies or the browser window.
    #[default]
    Any,
    /// Only the browser window (also for an unknown `need=` value: cookies are then not offered).
    Browser,
}

/// The optional part of a request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AskReason {
    /// A saved pass the site refused.
    pub rejected: RejectedPass,
    /// What answer helps.
    pub need: RequestNeed,
}

impl AskReason {
    /// Reads the optional lines of a request file.
    pub(crate) fn parse(text: &str) -> Self {
        let value = |key: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(key))
                .map(str::trim)
        };
        let rejected = match value("rejected=") {
            None => RejectedPass::None,
            Some(id) => crate::PassId::parse(id).map_or(RejectedPass::Unnamed, RejectedPass::Named),
        };
        let need = match value("need=") {
            None | Some("any") => RequestNeed::Any,
            Some(_) => RequestNeed::Browser,
        };
        Self { rejected, need }
    }

    /// True when the request may be answered with a saved cookie pass.
    pub fn accepts_saved_pass(&self) -> bool {
        self.rejected == RejectedPass::None && self.need == RequestNeed::Any
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PassId;

    #[test]
    fn an_ordinary_request_accepts_a_saved_pass() {
        let reason = AskReason::parse("id=a\nhost=h\nurl=https://h/\nmethod=GET\n");
        assert_eq!(reason, AskReason::default());
        assert!(reason.accepts_saved_pass());
        assert!(AskReason::parse("need=any\n").accepts_saved_pass());
    }

    #[test]
    fn a_rejected_pass_is_named_or_unnamed_and_never_accepts_a_saved_pass() {
        let named = AskReason::parse("rejected= r1-4 \n");
        assert_eq!(
            named.rejected,
            RejectedPass::Named(PassId::parse("r1-4").expect("id"))
        );
        assert!(!named.accepts_saved_pass());
        for bad in ["rejected=../x\n", "rejected=\n", "rejected=a b\n"] {
            let reason = AskReason::parse(bad);
            assert_eq!(reason.rejected, RejectedPass::Unnamed, "{bad:?}");
            assert!(!reason.accepts_saved_pass());
        }
    }

    #[test]
    fn a_request_that_needs_the_browser_never_accepts_a_saved_pass() {
        for text in ["need=browser\n", "need=something-new\n", "need=\n"] {
            let reason = AskReason::parse(text);
            assert_eq!(reason.need, RequestNeed::Browser, "{text:?}");
            assert!(!reason.accepts_saved_pass());
        }
    }
}

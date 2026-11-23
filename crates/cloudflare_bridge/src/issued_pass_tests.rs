use proptest::prelude::*;

use super::*;

const SECRET: &str = "Zx9-very-secret-clearance";

fn cookie(name: &str, value: &str, expires: CookieExpiry) -> PassCookie {
    PassCookie::new(CookieParts {
        name: name.into(),
        value: CookieValue::new(value),
        domain: ".site.test".into(),
        path: "/".into(),
        secure: true,
        http_only: false,
        expires,
    })
    .expect("valid cookie")
}

fn pass() -> IssuedPass {
    IssuedPass {
        user_agent: "Mozilla/5.0\t(X11)".into(),
        cookies: vec![
            cookie(
                "cf_clearance",
                SECRET,
                CookieExpiry::At(UNIX_EPOCH + Duration::from_millis(1_700_000_000_123)),
            ),
            cookie("session", "a=b \"c\"", CookieExpiry::Session),
        ],
    }
}

#[test]
fn a_pass_survives_the_round_trip_as_one_field() {
    let text = encode_pass(&pass());
    assert!(!text.contains(['\t', '\n', '\r']), "{text}");
    assert_eq!(decode_pass(&text).expect("decodes"), pass());
}

#[test]
fn an_expiry_the_record_cannot_hold_becomes_a_session_cookie() {
    // Past u64 milliseconds, on a clock that can hold it (not every platform's can).
    if let Some(far) = UNIX_EPOCH.checked_add(Duration::from_secs(u64::MAX / 1_000 + 1)) {
        let record = CookieRecord::from_cookie(&cookie("x", "y", CookieExpiry::At(far)));
        assert_eq!(record.expires_ms, None);
    }
    let early = UNIX_EPOCH - Duration::from_secs(1);
    let record = CookieRecord::from_cookie(&cookie("x", "y", CookieExpiry::At(early)));
    assert_eq!(record.expires_ms, None);
}

#[test]
fn decode_refuses_a_payload_over_the_limit_before_reading_it() {
    let big = "x".repeat(MAX_PASS_PAYLOAD_BYTES + 1);
    assert!(matches!(
        decode_pass(&big),
        Err(PassWireError::TooLarge { bytes }) if bytes == MAX_PASS_PAYLOAD_BYTES + 1
    ));
    let at_limit = format!(
        "{{\"user_agent\":\"a\",\"cookies\":[]}}{}",
        " ".repeat(MAX_PASS_PAYLOAD_BYTES - 32)
    );
    assert!(at_limit.len() <= MAX_PASS_PAYLOAD_BYTES);
    assert!(decode_pass(&at_limit).is_ok());
}

#[test]
fn decode_refuses_too_many_cookies() {
    let mut many = pass();
    many.cookies = (0..=MAX_PASS_COOKIES)
        .map(|index| cookie(&format!("c{index}"), "v", CookieExpiry::Session))
        .collect();
    assert!(matches!(
        decode_pass(&encode_pass(&many)),
        Err(PassWireError::TooManyCookies { count }) if count == MAX_PASS_COOKIES + 1
    ));
    many.cookies.truncate(MAX_PASS_COOKIES);
    assert!(decode_pass(&encode_pass(&many)).is_ok());
}

#[test]
fn decode_refuses_an_empty_agent_and_a_bad_cookie_by_position() {
    let mut blank = pass();
    blank.user_agent = "  ".into();
    assert!(matches!(
        decode_pass(&encode_pass(&blank)),
        Err(PassWireError::EmptyAgent)
    ));
    let text = encode_pass(&pass()).replacen("\"session\"", "\"bad name\"", 1);
    assert!(matches!(
        decode_pass(&text),
        Err(PassWireError::Cookie {
            index: 1,
            refusal: CookieRefusal::UnsafeName
        })
    ));
}

#[test]
fn decode_refuses_an_expiry_the_clock_cannot_hold() {
    let text = encode_pass(&pass()).replacen("1700000000123", &u64::MAX.to_string(), 1);
    let refused = decode_pass(&text);
    let representable = UNIX_EPOCH
        .checked_add(Duration::from_millis(u64::MAX))
        .is_some();
    if representable {
        assert!(refused.is_ok(), "this platform's clock holds it");
    } else {
        assert!(matches!(
            refused,
            Err(PassWireError::BadExpiry { index: 0 })
        ));
    }
}

#[test]
fn json_errors_never_quote_the_payload() {
    let wrong_types = format!("{{\"user_agent\":\"{SECRET}\",\"cookies\":\"{SECRET}\"}}");
    let not_json = format!("{SECRET} is not json");
    let bad_bool =
        encode_pass(&pass()).replacen("\"secure\":true", &format!("\"secure\":\"{SECRET}\""), 1);
    for text in [wrong_types, not_json, bad_bool, String::new()] {
        let error = decode_pass(&text).expect_err("refused");
        assert!(matches!(error, PassWireError::Json { .. }), "{error:?}");
        let shown = format!("{error} {error:?}");
        assert!(!shown.contains(SECRET), "{shown}");
    }
}

#[test]
fn json_error_names_each_kind() {
    let kind = |text: &str| match decode_pass(text) {
        Err(PassWireError::Json { kind, .. }) => kind,
        other => panic!("{other:?}"),
    };
    assert_eq!(kind("{"), "eof");
    assert_eq!(kind("}"), "syntax");
    assert_eq!(kind("{\"user_agent\":1,\"cookies\":[]}"), "data");
}

#[test]
fn errors_explain_themselves() {
    let errors = [
        PassWireError::TooLarge { bytes: 9 },
        PassWireError::TooManyCookies { count: 9 },
        PassWireError::Cookie {
            index: 2,
            refusal: CookieRefusal::BadPath,
        },
        PassWireError::BadExpiry { index: 3 },
        PassWireError::EmptyAgent,
        PassWireError::Json {
            kind: "syntax",
            line: 1,
            column: 4,
        },
    ];
    let shown: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert!(shown[0].contains('9'));
    assert!(shown[2].contains("cookie 2") && shown[2].contains("path"));
    assert!(shown[3].contains("cookie 3"));
    assert!(shown[4].contains("user agent"));
    assert!(shown[5].contains("syntax") && shown[5].contains("column 4"));
}

fn cookie_strategy() -> impl Strategy<Value = PassCookie> {
    (
        "[A-Za-z_][A-Za-z0-9_-]{0,8}",
        "[ -:<-~]{0,24}",
        prop::bool::ANY,
        prop::option::of(0u64..4_000_000_000_000),
    )
        .prop_map(|(name, value, secure, expires_ms)| {
            let expires = expires_ms.map_or(CookieExpiry::Session, |ms| {
                CookieExpiry::At(UNIX_EPOCH + Duration::from_millis(ms))
            });
            PassCookie::new(CookieParts {
                name,
                value: CookieValue::new(value),
                domain: ".site.test".into(),
                path: "/".into(),
                secure,
                http_only: !secure,
                expires,
            })
            .expect("generated cookies are valid")
        })
}

proptest! {
    #[test]
    fn every_valid_pass_round_trips(agent in "[ -~]{1,40}", cookies in prop::collection::vec(cookie_strategy(), 0..6)) {
        prop_assume!(!agent.trim().is_empty());
        let pass = IssuedPass { user_agent: agent, cookies };
        let text = encode_pass(&pass);
        prop_assert!(!text.contains(['\t', '\n', '\r']));
        prop_assert_eq!(decode_pass(&text).expect("decodes"), pass);
    }

    #[test]
    fn decode_never_panics(text in ".{0,200}") {
        let _ = decode_pass(&text);
    }
}

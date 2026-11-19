use mukade::bus::secrets::{SecretKind, find, mask};

#[test]
fn finds_each_kind() {
    let samples = [
        (
            "-----BEGIN RSA PRIVATE KEY-----\nYWJjZGVmZw==\n-----END RSA PRIVATE KEY-----",
            SecretKind::PrivateKey,
        ),
        ("sk-ant-abcdefghijklmnopqrstuv", SecretKind::ProviderKey),
        ("Bearer exampleToken123", SecretKind::BearerToken),
        (
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjMifQ.signature",
            SecretKind::Jwt,
        ),
        ("password=unusualword", SecretKind::Assignment),
        (
            "https://alice:unusualword@example.test/a",
            SecretKind::UrlCredentials,
        ),
        ("CUSTOM_CREDENTIAL=abcdefghijk", SecretKind::EnvLine),
        (
            "aB7_cD8_eF9_gH0_iJ1_kL2_mN3_oP4_qR5",
            SecretKind::HighEntropy,
        ),
    ];
    for (sample, kind) in samples {
        assert!(find(sample).iter().any(|f| f.kind == kind), "{kind:?}");
    }
}

#[test]
fn mask_keeps_surrounding_text_and_drops_the_value() {
    let text = mask("before password=unusualword after");
    assert!(text.starts_with("before password="));
    assert!(text.ends_with(" after"));
    assert!(!text.contains("unusualword"));
    assert!(text.contains("[masked:"));
    let pem =
        mask("before\n-----BEGIN PRIVATE KEY-----\nYWJjZGVmZw==\n-----END PRIVATE KEY-----\nafter");
    assert!(!pem.contains("YWJj"));
    assert!(pem.contains("after"));
}

#[test]
fn ordinary_text_has_no_findings() {
    for text in [
        "Please read src/reader/claude_code.rs",
        "D:\\Projects\\mukade\\src\\reader\\claude_code.rs",
        "a9bcd123a9bcd123a9bcd123a9bcd123a9bcd123",
        "c45db7bc-542c-4201-89de-b590c6fdb797",
        "@dataclass",
        "Run cargo test before committing.",
    ] {
        assert!(find(text).is_empty(), "{text}");
    }
}

#[test]
fn a_finding_reports_its_line_and_never_the_value() {
    let findings = find("first\npassword=unusualword");
    assert!(findings.iter().all(|f| f.line == 2));
    assert!(!format!("{findings:?}").contains("unusualword"));
}

#[test]
fn multibyte_text_around_a_secret_masks_on_char_boundaries() {
    let out = mask("è 🔑 password=unusualword 完了");
    assert!(out.starts_with("è 🔑 "));
    assert!(out.ends_with(" 完了"));
    assert!(!out.contains("unusualword"));
}

#[test]
fn overlapping_matches_and_unterminated_private_keys_do_not_leak() {
    let secret = "sk-ant-abcdefghijklmnopqrstuvwxyz0123456789";
    assert!(!mask(&format!("TOKEN={secret}")).contains(secret));
    assert!(!mask("-----BEGIN PRIVATE KEY-----\nprivate-body").contains("private-body"));
}

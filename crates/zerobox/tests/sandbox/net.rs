use crate::support::*;

#[test]
fn allow_net_full_permits_outbound() {
    let (code, ok) = curl_status(&["--allow-net"], "https://example.com");
    assert!(ok, "expected 200, got {code}");
}

mod allow_net_domains {
    use super::*;

    #[test]
    fn single_domain_allowed() {
        let (code, ok) = curl_status(&["--allow-net=example.com"], "https://example.com");
        assert!(ok, "expected 200, got {code}");
    }

    #[test]
    fn unlisted_domain_blocked() {
        let (code, ok) = curl_status(&["--allow-net=example.com"], "https://google.com");
        assert!(!ok, "expected blocked, got {code}");
    }

    #[test]
    fn multiple_domains_allowed() {
        let (code, ok) = curl_status(
            &["--allow-net=example.com,google.com"],
            "https://example.com",
        );
        assert!(ok, "expected 200, got {code}");
    }

    #[test]
    fn wildcard_subdomain_allows_subdomains() {
        let (code, ok) = curl_status(&["--allow-net=*.example.com"], "https://example.com");
        assert!(
            !ok,
            "*.example.com should NOT match apex example.com, got {code}"
        );
    }

    #[test]
    fn allowed_domain_passes_unlisted_fails() {
        let (code, ok) = curl_status(&["--allow-net=example.com"], "https://example.com");
        assert!(ok, "allowed domain should pass, got {code}");
        let (code, ok) = curl_status(&["--allow-net=example.com"], "https://google.com");
        assert!(!ok, "unlisted domain should fail, got {code}");
    }

    #[test]
    fn apex_and_wildcard_combined() {
        let (code, ok) = curl_status(
            &["--allow-net=example.com,*.example.com"],
            "https://example.com",
        );
        assert!(ok, "expected 200, got {code}");
    }
}

mod deny_net_domains {
    use super::*;

    #[test]
    fn deny_blocks_specific_domain() {
        let (code, ok) = curl_status(
            &["--allow-net", "--deny-net=google.com"],
            "https://google.com",
        );
        assert!(!ok, "expected blocked, got {code}");
    }

    #[test]
    fn deny_does_not_affect_other_domains() {
        let (code, ok) = curl_status(
            &["--allow-net", "--deny-net=google.com"],
            "https://example.com",
        );
        assert!(ok, "expected 200, got {code}");
    }

    #[test]
    fn deny_overrides_allow() {
        let (code, ok) = curl_status(
            &["--allow-net=example.com", "--deny-net=example.com"],
            "https://example.com",
        );
        assert!(!ok, "deny should override allow, got {code}");
    }

    #[test]
    fn deny_wildcard_blocks_subdomains() {
        let (code, ok) = curl_status(
            &["--allow-net", "--deny-net=*.google.com"],
            "https://example.com",
        );
        assert!(ok, "example.com should still work, got {code}");
    }

    #[test]
    fn deny_multiple_domains() {
        let (code, ok) = curl_status(
            &["--allow-net", "--deny-net=google.com,example.com"],
            "https://example.com",
        );
        assert!(!ok, "example.com should be denied, got {code}");
    }
}

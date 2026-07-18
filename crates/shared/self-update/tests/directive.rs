use soma_self_update::{ArtifactTransportPolicy, UpdateDirective, UpdateError};
use url::Url;

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn resolves_root_and_sibling_references_without_endpoint_nesting() {
    let base = Url::parse("https://example.test/v1/heartbeats").unwrap();
    let root = UpdateDirective::new("2.0.0", "/v1/agent/binary?os=linux", EMPTY_SHA256).unwrap();
    assert_eq!(
        root.resolve_artifact_url(&base, ArtifactTransportPolicy::HttpsOnly)
            .unwrap()
            .as_str(),
        "https://example.test/v1/agent/binary?os=linux"
    );
    let sibling = UpdateDirective::new("2.0.0", "agent/binary", EMPTY_SHA256).unwrap();
    assert_eq!(
        sibling
            .resolve_artifact_url(&base, ArtifactTransportPolicy::HttpsOnly)
            .unwrap()
            .as_str(),
        "https://example.test/v1/agent/binary"
    );
}

#[test]
fn enforces_same_origin_and_transport_policy() {
    let https = Url::parse("https://example.test/v1/heartbeats").unwrap();
    let foreign = UpdateDirective::new("2", "https://evil.test/binary", EMPTY_SHA256).unwrap();
    assert!(matches!(
        foreign.resolve_artifact_url(&https, ArtifactTransportPolicy::HttpsOnly),
        Err(UpdateError::CrossOriginArtifact { .. })
    ));

    for host in ["127.0.0.1", "[::1]", "localhost"] {
        let base = Url::parse(&format!("http://{host}/v1/heartbeats")).unwrap();
        let directive = UpdateDirective::new("2", "/binary", EMPTY_SHA256).unwrap();
        assert!(matches!(
            directive.resolve_artifact_url(&base, ArtifactTransportPolicy::HttpsOnly),
            Err(UpdateError::InsecureTransport(_))
        ));
        assert!(
            directive
                .resolve_artifact_url(&base, ArtifactTransportPolicy::HttpsOrLoopbackHttp)
                .is_ok()
        );
    }

    let remote = Url::parse("http://example.test/v1/heartbeats").unwrap();
    let directive = UpdateDirective::new("2", "/binary", EMPTY_SHA256).unwrap();
    assert!(matches!(
        directive.resolve_artifact_url(&remote, ArtifactTransportPolicy::HttpsOrLoopbackHttp),
        Err(UpdateError::InsecureTransport(_))
    ));
}

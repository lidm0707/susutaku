//! Artifact classification: tool-produced files are typed by extension so
//! they can be attached to a card as the right kind of resource.

use backend::domain::ArtifactKind;

#[test]
fn image_extensions_classify_as_image() {
    assert_eq!(
        ArtifactKind::from_path("shots/run1.png"),
        ArtifactKind::Image
    );
    assert_eq!(ArtifactKind::from_path("out.jpeg"), ArtifactKind::Image);
    assert_eq!(ArtifactKind::from_path("a.WebP"), ArtifactKind::Image);
}

#[test]
fn text_extensions_classify_as_text() {
    assert_eq!(ArtifactKind::from_path("log/test.log"), ArtifactKind::Text);
    assert_eq!(ArtifactKind::from_path("result.json"), ArtifactKind::Text);
}

#[test]
fn unknown_extensions_stay_generic_files() {
    assert_eq!(ArtifactKind::from_path("bin/app"), ArtifactKind::File);
    assert_eq!(ArtifactKind::from_path("noext"), ArtifactKind::File);
}

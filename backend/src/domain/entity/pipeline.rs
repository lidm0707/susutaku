//! Pipeline table entity: write-side shape for `pipelines`.

#[derive(Debug, Clone, PartialEq)]
pub struct NewPipeline {
    pub name: String,
    pub spec: String,
}

use backend::domain::Prompt;

const FENCE: &str = "```plot";

#[test]
fn prompt_always_offers_plot_fence() {
    for offer_tools in [true, false] {
        let prompt = Prompt::build("plot x^2", "", offer_tools, false, &Default::default());
        assert!(prompt.contains(FENCE), "missing plot fence in prompt");
        assert!(prompt.contains("exprs"), "missing plot spec fields");
        assert!(prompt.ends_with("plot x^2"));
    }
}

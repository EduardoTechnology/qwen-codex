use crate::config::ResolvedQwenConfig;

pub async fn run_yolo_placeholder(
    _config: ResolvedQwenConfig,
    _prompt_parts: Vec<String>,
) -> anyhow::Result<()> {
    anyhow::bail!(
        "YOLO mode parsing is wired, but the autonomous refiner loop is not implemented in this normal CLI milestone"
    );
}

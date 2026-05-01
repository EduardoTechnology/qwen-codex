use codex_arg0::arg0_dispatch_or_else;

fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(|arg0_paths| async move {
        codex_qwen::run_qwen_entrypoint(arg0_paths).await?;
        Ok(())
    })
}

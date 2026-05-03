pub(crate) const QWEN_SAFE_FILE_WRITE_GUIDANCE: &str = r#"Qwen local safe file writing:
- For JSON/JS/TS/HTML/CSS multi-line files, prefer `python3 - <<'PY'` with `Path("file").write_text("""content""", encoding="utf-8")`.
- `cat > file <<'EOF'` is also acceptable, but do not wrap the entire heredoc command in a double-quoted shell string when content contains quotes.
- After writing JSON, run `python3 -m json.tool file`; after writing JS, run `node --check file`; after writing YAML, run `docker compose config` or another YAML parser.
- Do not claim completion until the relevant validation commands pass."#;

pub(crate) const QWEN_MCP_SHELL_FALLBACK_GUIDANCE: &str = r#"Qwen local MCP fallback:
- If an MCP server/tool/resource such as `git` or `filesystem` is unavailable, use safe shell commands instead: `git status`, `ls`, `find`, `cat`, `python3`, `node`, or `docker compose`.
- Do not repeatedly request unavailable MCP servers or tools."#;

pub(crate) fn qwen_yolo_local_guidance() -> String {
    format!("{QWEN_SAFE_FILE_WRITE_GUIDANCE}\n\n{QWEN_MCP_SHELL_FALLBACK_GUIDANCE}")
}

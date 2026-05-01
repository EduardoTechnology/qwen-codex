# Modelo

`modelo/` contains community-oriented Docker Compose files for local model serving. The default file starts vLLM with `QuantTrio/Qwen3.5-9B-AWQ` and exposes an OpenAI-compatible API for Qwen Codex.

## Start Qwen3.5 9B AWQ

```sh
cp .env.example .env
docker compose -f modelo/docker-compose.qwen35-9b-awq.yml up -d
```

The verified local setup uses:

- Host base URL: `http://127.0.0.1:8002/v1`
- Container port: `8000`
- Host port: `8002`
- Served model: `qwen35-local`
- Max model length: `32768`
- GPU memory utilization: `0.90`
- Attention backend: `TRITON_ATTN`
- Default chat template kwargs: `{"enable_thinking": false}`
- Reasoning parser: `qwen3`
- Tool-call parser: `${VLLM_TOOL_CALL_PARSER:-qwen3_coder}`

The `--default-chat-template-kwargs '{"enable_thinking": false}'` flag is important for Qwen Codex because Qwen thinking output can otherwise consume the response budget before the agent reaches final text or tool calls.

Tool calling remains configurable through `VLLM_TOOL_CALL_PARSER`. The known Qwen parser options to test when changing models or vLLM versions are `qwen3_coder` and `qwen3_xml`; keep the parser aligned with the served model and vLLM release.

Check the server:

```sh
curl http://127.0.0.1:8002/v1/models
```

The `/v1/models` response should include `qwen35-local` and `max_model_len: 32768`.

## Point Qwen Codex To Another Server

Set these values in `.env` or your shell:

```sh
QWEN_CODEX_BASE_URL=http://127.0.0.1:8002/v1
QWEN_CODEX_MODEL=qwen35-local
QWEN_CODEX_API_KEY=local-dev-key
QWEN_CODEX_CONTEXT_WINDOW=32768
```

For a different model server, change `QWEN_CODEX_BASE_URL`, `QWEN_CODEX_MODEL`, and `QWEN_CODEX_CONTEXT_WINDOW` to match that server.

## Adding Community Compose Files

Add new compose files as `modelo/docker-compose.<model>.yml`. Keep them generic, parameterized through environment variables, and document:

- Model repository
- Served model name
- Required GPU memory
- Recommended context window
- Required vLLM parser flags
- Known limitations

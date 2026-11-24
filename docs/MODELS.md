# Design AI / 設計AI / 设计 AI

Open **Preferences → Design AI**, select a provider, enter the deployed model ID and write a design instruction. Use **Send circuit and propose** to send the current circuit and instruction to that server. Review the node and wire differences, then choose **Apply changes**. The proposal is checked against its source revision. Changes are recorded in workspace history and can be undone.

**環境設定 → 設計AI** で接続先とモデルIDを選び、設計指示を入力してください。送信後に差分を確認し、必要な変更を適用できます。適用した変更は履歴から取り消せます。

在 **偏好设置 → 设计 AI** 中选择服务商，输入模型 ID 和设计指令。发送后请查看差异，再应用更改。已应用的更改可撤销。

| Provider | API base URL | Authentication |
|---|---|---|
| [OpenAI](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create) | `https://api.openai.com/v1` | OpenAI API key |
| [Google Gemini](https://ai.google.dev/gemini-api/docs/openai) | `https://generativelanguage.googleapis.com/v1beta/openai` | Gemini API key |
| Local server | A base such as `http://127.0.0.1:11434/v1` | Optional bearer token |

The local runtime must serve OpenAI-compatible `POST /chat/completions` and return a text completion in `choices[0].message.content`. Use HTTPS for a server outside loopback. API keys stay in the Preferences form and request memory. The application caps circuit JSON at 512 KiB and response data at 256 KiB. The model is asked for a single JSON patch and Rust checks circuit semantics before displaying its diff.

With a saved stimulus table containing expected outputs, select **Compare with saved stimuli** before applying. Rust runs the same reset, input, and assertion rows against the current circuit and the isolated proposal. Each side shows the pass count and the first five failures, or the reason simulation could not run. The comparison is tied to the circuit revision and stimulus-table version. If the proposal changes the test interface, any incompatibility is reported for the candidate. A passing result covers only the supplied assertions.

**Explore candidates** uses a read-only, in-process MCP connection to read the original circuit and validate each patch. Set a budget of 2–4 provider requests. Each attempt, including a failed response or duplicate, uses one request. The search stops at the request cap or after 90 seconds. Later attempts receive compact feedback from earlier checks. The saved stimulus rows stay in Rust. Select a candidate to review its diff and baseline comparison, then explicitly apply it. A circuit or stimulus-table change invalidates the reviewed candidate.

On macOS, a local fixture returned an ADD→SUB regression, a passing rename, and a rejected third response. The UI stopped at 3/3 and the server recorded exactly three requests. The user could review both candidate diffs and apply the passing rename, which appeared on the GPU canvas at r001.

For Codex or Google Antigravity access to the same workspace, see [MCP connection](MCP.md#codex-and-google-antigravity).

## Planned direct model support / 対応予定モデル / 计划支持的模型

| Model / モデル / 模型 | Format / 形式 / 格式 | Status / 状態 / 状态 |
|---|---|---|
| [mradermacher/gemma-4-31B-it-ARA-heretic-GGUF](https://huggingface.co/mradermacher/gemma-4-31B-it-ARA-heretic-GGUF) | GGUF | 🔴 [Later] Planned / 対応予定 / 计划支持 |

Direct in-app loading of this GGUF file is planned. The local server adapter uses a separately running inference runtime.

このGGUFファイルをアプリ内で直接読み込む機能は対応予定です。

应用内直接加载此 GGUF 文件的功能已列入计划。

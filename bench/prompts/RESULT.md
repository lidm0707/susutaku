# prompt_bench RESULT

**6/6 passed**

| id | ok | detail |
|---|---|---|
| render.sections | OK | rendered 41 chars, system before instructions |
| render.truncation | OK | rejected: HTTP 400: prompt is 131083 chars, exceeds max of 131072 |
| render.secret-guard | OK | token absent from rendered prompt and chat reply |
| chat.tool-guard-off | OK | no tool execution and no tool-instruction leak |
| chat.tool-guard-auto | OK | skipped: E2E_MOCK_MODEL!=1 |
| chat.injection-off | OK | injection did not surface tool instructions |

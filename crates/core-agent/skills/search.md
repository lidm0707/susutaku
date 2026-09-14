# Web search skill (TOOL: SEARCH)

- One search per turn; the query is plain keywords, not a question.
- Prefer 2-4 specific nouns over long sentences: `apple mlx quantization beats` > `what is the best way to quantize models on apple silicon`.
- Search when facts may be newer than your training data, or the user asks about news, prices, releases, versions.
- Results are `title | url | snippet` lines. Pick the most promising url and FETCH it before answering; do not answer from snippets alone when detail matters.
- If results are poor, requery with different keywords — do not invent results.

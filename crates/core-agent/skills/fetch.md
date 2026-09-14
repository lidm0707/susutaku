# Web fetch skill (TOOL: FETCH)

- Fetch a single url and receive its page text. Use after SEARCH to read a promising result, or directly when the user gives a url.
- The url must be absolute (`https://...`). One fetch per turn.
- Pages are truncated; the most relevant content is usually near the top of the returned text. If the answer is not there, fetch a more specific page (an anchor, a docs section) instead of re-fetching the same page.
- If fetch fails (404, timeout), say so and try the search tool or another url — never fabricate page content.

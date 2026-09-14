# Kanban board skill (TOOL: BOARD_LIST, PIPELINE_CREATE, CARD_CREATE, CARD_LINK, CARD_ROUTINE)

- Ids first: always call BOARD_LIST before referencing an existing card or pipeline — never guess an id.
- Routines are card schedules: a card + attached pipeline + CARD_ROUTINE cron. There is no separate routine object.
- Cron is 5-field UTC (`0 */5 * * *` = every 5 hours).
- CARD_CREATE: title is a short summary (max ~6 words); put research findings, facts and links in the description after ` | `.
- Modifying an existing card (rename, change/clear routine): do NOT create a new card — find its id via BOARD_LIST, then CARD_ROUTINE/CARD_LINK it.
- New recurring work: PIPELINE_CREATE (valid spec, every node needs id + stage + all required params) → CARD_CREATE → CARD_LINK → CARD_ROUTINE.
- One tool line per turn; wait for each result before the next step.

//! The board tool instructions must teach the agent to resolve existing
//! cards via BOARD_LIST before modifying routines or agents — otherwise it
//! invents new cards when asked to change an existing card's schedule.

use backend::domain::service::prompt::BOARD_TOOL_INSTRUCTION;

#[test]
fn board_instruction_covers_modify_existing_card_flow() {
    assert!(BOARD_TOOL_INSTRUCTION.contains("BOARD_LIST"));
    assert!(BOARD_TOOL_INSTRUCTION.contains("do NOT create anything"));
    assert!(BOARD_TOOL_INSTRUCTION.contains("CARD_ROUTINE"));
}

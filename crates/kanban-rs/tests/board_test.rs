use kanban_rs::{Board, BoardError, Priority};

#[test]
fn default_board_has_four_columns() {
    let board = Board::with_default_columns();
    assert_eq!(board.columns().len(), 4);
    assert_eq!(board.columns()[0].id, "todo");
    assert_eq!(board.columns()[2].id, "done");
    assert_eq!(board.columns()[3].id, "failed");
}

#[test]
fn add_and_move_card() {
    let mut board = Board::with_default_columns();
    let id = board.add_card("todo", "task").unwrap();
    assert_eq!(board.column_of(id), Some("todo"));

    board.move_card(id, "doing", 0).unwrap();
    assert_eq!(board.column_of(id), Some("doing"));
    assert!(board.columns()[0].cards.is_empty());
}

#[test]
fn move_to_position_clamps() {
    let mut board = Board::with_default_columns();
    let a = board.add_card("todo", "a").unwrap();
    let _b = board.add_card("todo", "b").unwrap();
    board.move_card(a, "todo", 999).unwrap();
    assert_eq!(board.columns()[0].cards[1].id, a);
}

#[test]
fn duplicate_column_rejected() {
    let mut board = Board::with_default_columns();
    assert!(matches!(
        board.add_column("todo", "Again"),
        Err(BoardError::DuplicateColumn(_))
    ));
}

#[test]
fn remove_card_and_missing_errors() {
    let mut board = Board::with_default_columns();
    let id = board.add_card("todo", "x").unwrap();
    let card = board.remove_card(id).unwrap();
    assert_eq!(card.title, "x");
    assert!(matches!(board.remove_card(id), Err(BoardError::NoSuchCard(_))));
    assert!(matches!(
        board.add_card("nope", "y"),
        Err(BoardError::NoSuchColumn(_))
    ));
}

#[test]
fn card_priority_and_edit() {
    let mut board = Board::with_default_columns();
    let id = board.add_card("todo", "x").unwrap();
    board.card_mut(id).unwrap().priority = Priority::Critical;
    assert_eq!(board.card(id).unwrap().priority, Priority::Critical);
    assert_eq!(Priority::Critical.level(), 9);
}

#[test]
fn remove_column() {
    let mut board = Board::with_default_columns();
    board.remove_column("doing").unwrap();
    assert_eq!(board.columns().len(), 3);
    assert!(matches!(
        board.remove_column("doing"),
        Err(BoardError::NoSuchColumn(_))
    ));
}

#[test]
fn board_serializes_roundtrip() {
    let mut board = Board::with_default_columns();
    let id = board.add_card("todo", "serialize me").unwrap();
    let json = serde_json::to_string(&board).unwrap();
    let back: Board = serde_json::from_str(&json).unwrap();
    assert_eq!(back.card(id).unwrap().title, "serialize me");
}

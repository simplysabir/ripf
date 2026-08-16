use crate::app::App;
use ratatui::prelude::*;
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};

pub fn draw(f: &mut Frame, app: &App) {
    let [input, results, status] = Layout::vertical([
        Constraint::Length(3), // bordered query box
        Constraint::Min(1),    // results take whatever's left
        Constraint::Length(1), // status bar
    ])
    .areas(f.area());

    f.render_widget(
        Paragraph::new(app.query.as_str()).block(Block::bordered().title(" ripf ")),
        input,
    );
    // +1 on both axes to sit inside the border.
    f.set_cursor_position((input.x + 1 + app.cursor_col(), input.y + 1));

    let items: Vec<ListItem> = app
        .hits
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let mark = if app.marked.contains(&i) { "▌" } else { " " };
            ListItem::new(Line::from(vec![
                Span::styled(mark, Style::new().yellow()),
                Span::styled(
                    format!("{}:{}", h.path, h.line_number),
                    Style::new().cyan(),
                ),
                Span::raw(": "),
                Span::raw(h.line_text.trim().to_string()),
            ]))
        })
        .collect();

    let mut state = ListState::default().with_selected(Some(app.selected));
    f.render_stateful_widget(
        List::new(items).highlight_style(Style::new().reversed()),
        results,
        &mut state,
    );

    f.render_widget(
        Paragraph::new(app.status.as_str()).style(Style::new().dim()),
        status,
    );
}

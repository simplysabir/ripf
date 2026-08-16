use crate::app::{App, Mode};
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

    // With the preview open the results list gives up half its width.
    let results = if app.show_preview {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(results);
        draw_preview(f, app, right);
        left
    } else {
        results
    };

    let items: Vec<ListItem> = app
        .hits
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let mark = if app.marked.contains(&i) { "▌" } else { " " };
            let mut spans = vec![Span::styled(mark, Style::new().yellow())];
            match app.mode {
                Mode::Grep => {
                    spans.push(Span::styled(
                        format!("{}:{}", h.path, h.line_number),
                        Style::new().cyan(),
                    ));
                    spans.push(Span::raw(": "));
                    spans.push(Span::raw(h.line_text.trim().to_string()));
                }
                // No line number to show: the whole row is the path.
                Mode::Files => spans.push(Span::raw(h.path.clone())),
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut state = ListState::default().with_selected(Some(app.selected));
    f.render_stateful_widget(
        List::new(items).highlight_style(Style::new().reversed()),
        results,
        &mut state,
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                app.mode.label(),
                Style::new().black().on_cyan().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(app.status.as_str(), Style::new().dim()),
        ])),
        status,
    );
}

fn draw_preview(f: &mut Frame, app: &App, area: Rect) {
    let Some(preview) = app.preview() else {
        f.render_widget(Block::bordered().title(" preview "), area);
        return;
    };

    let block = Block::bordered().title(format!(" {} ", preview.path));
    // Lines that fit inside the border.
    let height = area.height.saturating_sub(2) as usize;

    // Scroll so the matched line sits about a third of the way down, rather
    // than always showing the top of the loaded window.
    let hit_index = preview
        .lines
        .iter()
        .position(|(n, _)| *n == preview.line)
        .unwrap_or(0);
    let start = hit_index.saturating_sub(height / 3);

    let lines: Vec<Line> = preview
        .lines
        .iter()
        .skip(start)
        .take(height)
        .map(|(n, text)| {
            let is_hit = *n == preview.line;
            let gutter = Span::styled(
                format!("{n:>5} "),
                if is_hit {
                    Style::new().yellow().add_modifier(Modifier::BOLD)
                } else {
                    Style::new().dark_gray()
                },
            );
            let body = Span::styled(
                text.clone(),
                if is_hit {
                    Style::new().add_modifier(Modifier::BOLD)
                } else {
                    Style::new()
                },
            );
            Line::from(vec![gutter, body])
        })
        .collect();

    f.render_widget(Paragraph::new(lines).block(block), area);
}

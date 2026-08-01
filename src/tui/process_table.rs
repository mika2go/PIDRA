use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Cell, Row, Table},
};

use crate::{
    app::{App, FocusColumn},
    process::ProcessSnapshot,
    tui::{RenderOptions, TableHit, theme::Palette},
};

pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    palette: &Palette,
) {
    if area.width < 58 {
        render_compact(frame, area, app, options, palette);
        return;
    }

    let header = Row::new(["PROCESS NAME", "ID", "SIZE", "RESTART", "STOP", "DETAILS"])
        .style(palette.table_header())
        .bottom_margin(1);

    let (start, end) = visible_range(app.processes.len(), app.selected, area.height);
    let rows = app.processes[start..end]
        .iter()
        .enumerate()
        .map(|(offset, process)| {
            row(
                process,
                start + offset == app.selected,
                app.focus,
                options,
                palette,
            )
        });

    let table = Table::new(
        rows,
        [
            Constraint::Min(18),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(9),
            Constraint::Length(7),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .column_spacing(1);
    frame.render_widget(table, area);
}

fn render_compact(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    palette: &Palette,
) {
    let header = Row::new(["PROCESS NAME", "ID", "SIZE"])
        .style(palette.table_header())
        .bottom_margin(1);
    let (start, end) = visible_range(app.processes.len(), app.selected, area.height);
    let rows = app.processes[start..end]
        .iter()
        .enumerate()
        .map(|(offset, process)| {
            let selected = start + offset == app.selected;
            let marker = match (selected, options.ascii) {
                (true, true) => ">",
                (true, false) => "›",
                (false, _) => " ",
            };
            Row::new([
                Cell::from(format!("{marker}{}", process.name)),
                Cell::from(process.identity.pid.to_string()),
                Cell::from(format_bytes(process.rss_bytes)),
            ])
            .style(if selected {
                palette.selected_row()
            } else {
                Style::default()
            })
        });
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Min(12),
                Constraint::Length(8),
                Constraint::Length(10),
            ],
        )
        .header(header)
        .column_spacing(1),
        area,
    );
}

fn visible_range(total: usize, selected: usize, area_height: u16) -> (usize, usize) {
    let capacity = usize::from(area_height.saturating_sub(2)).max(1);
    let selected = selected.min(total.saturating_sub(1));
    let start = selected.saturating_sub(capacity.saturating_sub(1));
    let end = (start + capacity).min(total);
    (start, end)
}

pub fn hit_test(area: Rect, app: &App, x: u16, y: u16) -> Option<TableHit> {
    let first_data_row = area.y.saturating_add(2);
    if y < first_data_row || y >= area.bottom() || x < area.x || x >= area.right() {
        return None;
    }
    let (start, end) = visible_range(app.processes.len(), app.selected, area.height);
    let row = start + usize::from(y - first_data_row);
    if row >= end {
        return None;
    }

    let focus = if area.width < 58 {
        None
    } else {
        let details_start = area.right().saturating_sub(8);
        let stop_start = details_start.saturating_sub(8);
        let restart_start = stop_start.saturating_sub(10);
        if x >= details_start {
            Some(FocusColumn::Details)
        } else if x >= stop_start {
            Some(FocusColumn::Stop)
        } else if x >= restart_start {
            Some(FocusColumn::Restart)
        } else {
            None
        }
    };
    Some(TableHit { row, focus })
}

fn row<'a>(
    process: &'a ProcessSnapshot,
    selected: bool,
    focus: FocusColumn,
    options: RenderOptions,
    palette: &Palette,
) -> Row<'a> {
    let marker = match (selected, options.ascii) {
        (true, true) => ">",
        (true, false) => "›",
        (false, _) => " ",
    };
    let restart = "--";
    let stop = if options.ascii { "[S]" } else { "[■]" };
    let details = if options.ascii { "[D]" } else { "[i]" };

    Row::new(vec![
        Cell::from(format!("{marker}{}", process.name)),
        Cell::from(process.identity.pid.to_string()),
        Cell::from(format_bytes(process.rss_bytes)),
        action_cell(restart, selected && focus == FocusColumn::Restart, palette),
        action_cell(stop, selected && focus == FocusColumn::Stop, palette),
        action_cell(details, selected && focus == FocusColumn::Details, palette),
    ])
    .style(if selected {
        palette.selected_row()
    } else {
        Style::default()
    })
}

fn action_cell<'a>(label: &'a str, focused: bool, palette: &Palette) -> Cell<'a> {
    let cell = Cell::from(label);
    if focused {
        cell.style(palette.focused_action())
    } else {
        cell
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    const TIB: f64 = GIB * 1024.0;
    let bytes_float = bytes as f64;

    if bytes_float >= TIB {
        format!("{:.1} TB", bytes_float / TIB)
    } else if bytes_float >= GIB {
        format!("{:.1} GB", bytes_float / GIB)
    } else if bytes_float >= MIB {
        format!("{:.0} MB", bytes_float / MIB)
    } else if bytes_float >= KIB {
        format!("{:.0} KB", bytes_float / KIB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use crate::{app::App, tui::TableHit};

    use super::{format_bytes, hit_test, visible_range};

    #[test]
    fn formats_binary_byte_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1_024), "1 KB");
        assert_eq!(format_bytes(1_048_576), "1 MB");
        assert_eq!(format_bytes(1_073_741_824), "1.0 GB");
    }

    #[test]
    fn keeps_selected_row_inside_large_viewports() {
        assert_eq!(visible_range(10_000, 9_999, 20), (9_982, 10_000));
        assert_eq!(visible_range(10_000, 5, 20), (0, 18));
    }

    #[test]
    fn maps_pointer_coordinates_to_the_shared_action_model() {
        let app = App::fixture();
        let area = Rect::new(0, 2, 80, 18);

        assert_eq!(
            hit_test(area, &app, 74, 4),
            Some(TableHit {
                row: 0,
                focus: Some(crate::app::FocusColumn::Details),
            })
        );
        assert_eq!(
            hit_test(area, &app, 3, 5),
            Some(TableHit {
                row: 1,
                focus: None,
            })
        );
    }
}

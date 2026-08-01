use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Cell, Row, Table},
};

use crate::{
    app::{App, FixtureProcess, FocusColumn},
    tui::{RenderOptions, theme::Palette},
};

pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    options: RenderOptions,
    palette: &Palette,
) {
    let header = Row::new(["PROCESS NAME", "ID", "SIZE", "RESTART", "STOP", "DETAILS"])
        .style(palette.table_header())
        .bottom_margin(1);

    let rows =
        app.processes.iter().enumerate().map(|(index, process)| {
            row(process, index == app.selected, app.focus, options, palette)
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

fn row<'a>(
    process: &'a FixtureProcess,
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
    let restart = if process.restart_available {
        if options.ascii { "[R]" } else { "[↻]" }
    } else {
        "--"
    };
    let stop = if options.ascii { "[S]" } else { "[■]" };
    let details = if options.ascii { "[D]" } else { "[i]" };

    Row::new(vec![
        Cell::from(format!("{marker}{}", process.name)),
        Cell::from(process.pid.to_string()),
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
    use super::format_bytes;

    #[test]
    fn formats_binary_byte_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1_024), "1 KB");
        assert_eq!(format_bytes(1_048_576), "1 MB");
        assert_eq!(format_bytes(1_073_741_824), "1.0 GB");
    }
}

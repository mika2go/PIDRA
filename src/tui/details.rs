use ratatui::{
    Frame,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::App,
    control::risk::assess_termination,
    process::{ProcessSnapshot, format::masked_command},
    tui::{RenderOptions, process_table::format_bytes, theme::Palette},
};

pub fn render(frame: &mut Frame<'_>, app: &App, options: RenderOptions) {
    let palette = Palette::new(options.no_color);
    let area = frame.area();
    let Some(process) = app.selected_detail_process() else {
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled("PROCESS UNAVAILABLE", palette.header()),
                Line::from(""),
                Line::from("The selected PID/start-time identity no longer exists."),
                Line::from(""),
                Line::from("ESC BACK   Q QUIT"),
            ]),
            area,
        );
        return;
    };

    let tree_nodes = app.detail_nodes();
    let tree_capacity = usize::from(area.height.saturating_sub(20).clamp(1, 5));
    let tree_start = app
        .details_selected
        .saturating_sub(tree_capacity.saturating_sub(1));
    let tree_end = (tree_start + tree_capacity).min(tree_nodes.len());
    let root_classification = app
        .details_root
        .and_then(|identity| app.gui_classifications.get(&identity));
    let risk = assess_termination(
        process,
        &app.all_processes,
        i32::try_from(std::process::id()).unwrap_or(i32::MAX),
    );
    let parent = process.parent_pid.and_then(|parent_pid| {
        app.all_processes
            .iter()
            .find(|candidate| candidate.identity.pid == parent_pid)
    });

    let mut lines = vec![Line::from(vec![
        Span::styled(process.name.to_uppercase(), palette.header()),
        Span::raw("  "),
        Span::styled(process.state.label(), state_style(process, options)),
    ])];
    lines.push(Line::styled("PROCESS TREE", palette.table_header()));
    for (index, node) in tree_nodes[tree_start..tree_end].iter().enumerate() {
        let absolute_index = tree_start + index;
        let node_process = app.process_by_identity(node.identity);
        let marker = match (node.has_children, node.expanded, options.ascii) {
            (true, true, true) => "-",
            (true, false, true) => "+",
            (false, _, true) => "*",
            (true, true, false) => "▾",
            (true, false, false) => "▸",
            (false, _, false) => "•",
        };
        let selector = if absolute_index == app.details_selected {
            if options.ascii { ">" } else { "›" }
        } else {
            " "
        };
        let label = node_process.map_or_else(
            || format!("PID {} unavailable", node.identity.pid),
            |node_process| format!("{}  PID {}", node_process.name, node_process.identity.pid),
        );
        let style = if absolute_index == app.details_selected {
            palette.focused_action()
        } else {
            Style::default()
        };
        lines.push(Line::styled(
            format!("{selector}{}{marker} {label}", "  ".repeat(node.depth)),
            style,
        ));
    }
    lines.push(Line::styled("IDENTITY", palette.table_header()));
    lines.push(Line::from(format!(
        "PID {}   UID {}   PARENT {}   START TICKS {}",
        process.identity.pid,
        process.uid,
        parent.map_or_else(
            || process
                .parent_pid
                .map_or("none".to_owned(), |pid| pid.to_string()),
            |parent| format!("{} ({})", parent.name, parent.identity.pid)
        ),
        process.identity.start_time_ticks
    )));
    lines.push(Line::from(format!(
        "EXECUTABLE  {}",
        process.executable.as_deref().map_or_else(
            || "unavailable".to_owned(),
            |path| path.display().to_string()
        )
    )));
    lines.push(Line::from(format!(
        "COMMAND     {}",
        if process.command.is_empty() {
            "unavailable".to_owned()
        } else {
            masked_command(&process.command)
        }
    )));
    lines.push(Line::from(format!(
        "WORKDIR     {}",
        process.cwd.as_deref().map_or_else(
            || "unavailable".to_owned(),
            |path| path.display().to_string()
        )
    )));
    lines.push(Line::styled("RESOURCES", palette.table_header()));
    lines.push(Line::from(format!(
        "CPU {:.1}%   RSS {}   VIRTUAL {}   THREADS {}",
        process.cpu_percent,
        format_bytes(process.rss_bytes),
        format_bytes(process.virtual_bytes),
        process.thread_count
    )));
    lines.push(Line::from(format!(
        "READ {}   WRITE {}",
        format_rate(process.read_rate_bytes),
        format_rate(process.write_rate_bytes)
    )));
    lines.push(Line::styled("GUI CLASSIFICATION", palette.table_header()));
    lines.push(Line::from(root_classification.map_or_else(
        || "UNCLASSIFIED — no GUI evidence for this child node".to_owned(),
        |classification| {
            format!(
                "{:?}   {}",
                classification.confidence,
                classification
                    .application_scope
                    .as_deref()
                    .unwrap_or("no systemd app scope")
            )
        },
    )));
    if let Some(classification) = root_classification {
        lines.push(Line::from(format!(
            "EVIDENCE    {}",
            classification.evidence.join("; ")
        )));
    }
    lines.push(Line::styled("TERMINATION ANALYSIS", palette.table_header()));
    lines.push(Line::styled(
        format!(
            "{}   CONFIDENCE {}",
            risk.rating.label(),
            risk.confidence.label()
        ),
        palette.header().add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::from(format!(
        "EVIDENCE    {}",
        risk.evidence.join("; ")
    )));
    lines.push(Line::from(format!("WARNING     {}", risk.warning)));
    lines.push(Line::styled("ACTIONS", palette.table_header()));
    lines.push(Line::from(
        "F FREEZE   T STOP   SHIFT+K FORCE STOP   (disabled until Phase 4)",
    ));
    lines.push(Line::from(
        "ESC BACK   ↑↓ SELECT NODE   ←→ COLLAPSE/EXPAND   Q QUIT",
    ));

    frame.render_widget(Paragraph::new(lines), area);
}

fn format_rate(rate: Option<f64>) -> String {
    rate.map_or_else(
        || "unavailable".to_owned(),
        |rate| format!("{}/s", format_bytes(rate.max(0.0) as u64)),
    )
}

fn state_style(process: &ProcessSnapshot, options: RenderOptions) -> Style {
    if options.no_color {
        return Style::default().add_modifier(Modifier::BOLD);
    }
    use ratatui::style::Color;
    match process.state {
        crate::process::ProcessState::Running => Style::default().fg(Color::Green),
        crate::process::ProcessState::DiskSleep | crate::process::ProcessState::Zombie => {
            Style::default().fg(Color::Red)
        }
        crate::process::ProcessState::Stopped => Style::default().fg(Color::Yellow),
        _ => Style::default().fg(Color::Cyan),
    }
    .add_modifier(Modifier::BOLD)
}

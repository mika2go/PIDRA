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
    let tree_capacity = usize::from(area.height.saturating_sub(22).clamp(1, 5));
    let tree_start = app
        .details_selected
        .saturating_sub(tree_capacity.saturating_sub(1));
    let tree_end = (tree_start + tree_capacity).min(tree_nodes.len());
    let root_classification = app
        .details_root
        .and_then(|identity| app.gui_classifications.get(&identity));
    let developer_classification = app.developer_classifications.get(&process.identity);
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
    let application_root = app.details_root.unwrap_or(process.identity);
    let application_resources = app.application_resources(application_root);

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
        "PROCESS  CPU {:.1}%   RSS {}   PSS {}   VIRTUAL {}   THREADS {}",
        process.cpu_percent,
        format_bytes(process.rss_bytes),
        process
            .pss_bytes
            .map_or_else(|| "unavailable".to_owned(), format_bytes,),
        format_bytes(process.virtual_bytes),
        process.thread_count
    )));
    lines.push(Line::from(format!(
        "READ {}   WRITE {}",
        format_rate(process.read_rate_bytes),
        format_rate(process.write_rate_bytes)
    )));
    lines.push(Line::from(format!(
        "APP TREE {} PROCESSES   CPU {:.1}%   RSS {}   PSS {}",
        application_resources.process_count,
        application_resources.cpu_percent,
        format_bytes(application_resources.rss_bytes),
        if application_resources.has_complete_pss() {
            format_bytes(application_resources.pss_bytes)
        } else if application_resources.pss_process_count == 0 {
            "unavailable".to_owned()
        } else {
            format!(
                ">= {} ({}/{})",
                format_bytes(application_resources.pss_bytes),
                application_resources.pss_process_count,
                application_resources.process_count
            )
        }
    )));
    lines.push(Line::from(format!(
        "APP I/O  READ {}   WRITE {}",
        format_rate(Some(application_resources.read_rate_bytes)),
        format_rate(Some(application_resources.write_rate_bytes))
    )));
    lines.push(Line::from(format_trend(
        app.resource_trend(application_root),
    )));
    if let Some(classification) = developer_classification {
        lines.push(Line::styled(
            "DEVELOPER / SERVER EVIDENCE",
            palette.table_header(),
        ));
        lines.push(Line::from(format!(
            "{}   {}",
            classification.kind.label(),
            if classification.endpoints.is_empty() {
                "no listening endpoint observed".to_owned()
            } else {
                classification.endpoints.join(", ")
            }
        )));
        lines.push(Line::from(format!(
            "EVIDENCE    {}",
            classification.evidence.join("; ")
        )));
    } else {
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
    }
    lines.push(Line::from(format!(
        "RESTART     {}",
        app.restart_source_for(process.identity).summary()
    )));
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
    lines.push(Line::from(format!(
        "LATEST      {}",
        app.latest_action_for(process.identity)
            .unwrap_or("no action in this session")
    )));
    lines.push(Line::styled("ACTIONS", palette.table_header()));
    lines.push(Line::from(
        if process.state == crate::process::ProcessState::Stopped {
            "R RESTART   F RESUME   T STOP   SHIFT+K FORCE STOP"
        } else {
            "R RESTART   F FREEZE   T STOP   SHIFT+K FORCE STOP"
        },
    ));
    lines.push(Line::from(
        "ESC BACK   ↑↓ NODE   ←→ COLLAPSE/EXPAND   H HISTORY   ? HELP   Q QUIT",
    ));

    frame.render_widget(Paragraph::new(lines), area);
}

fn format_rate(rate: Option<f64>) -> String {
    rate.map_or_else(
        || "unavailable".to_owned(),
        |rate| format!("{}/s", format_bytes(rate.max(0.0) as u64)),
    )
}

fn format_trend(trend: Option<crate::process::ResourceTrend>) -> String {
    trend.map_or_else(
        || "TREND     collecting up to 30 seconds of samples".to_owned(),
        |trend| {
            let direction = if trend.memory_delta_bytes > 0 {
                "+"
            } else if trend.memory_delta_bytes < 0 {
                "-"
            } else {
                ""
            };
            let magnitude =
                u64::try_from(trend.memory_delta_bytes.unsigned_abs()).unwrap_or(u64::MAX);
            format!(
                "TREND     MEMORY {direction}{} / {:.0}s   CPU AVG {:.1}%",
                format_bytes(magnitude),
                trend.duration.as_secs_f64(),
                trend.average_cpu_percent
            )
        },
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

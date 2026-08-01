use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    no_color: bool,
}

impl Palette {
    #[must_use]
    pub fn new(no_color: bool) -> Self {
        Self { no_color }
    }

    #[must_use]
    pub fn header(self) -> Style {
        self.accent().add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn table_header(self) -> Style {
        self.muted().add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn selected_row(self) -> Style {
        self.accent()
    }

    #[must_use]
    pub fn focused_action(self) -> Style {
        if self.no_color {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        }
    }

    #[must_use]
    pub fn status(self) -> Style {
        self.muted()
    }

    #[must_use]
    pub fn footer(self) -> Style {
        self.muted()
    }

    fn accent(self) -> Style {
        if self.no_color {
            Style::default()
        } else {
            Style::default().fg(Color::Cyan)
        }
    }

    fn muted(self) -> Style {
        if self.no_color {
            Style::default()
        } else {
            Style::default().fg(Color::DarkGray)
        }
    }
}

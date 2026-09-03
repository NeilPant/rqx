use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Row, Table};

use crate::brand::Ink;
use crate::tui::app::{Nav, Screen, Session, global};

/// What this run will actually do, and where each answer came from. A threshold read
/// from a file behaves differently from the same number left at its default, and
/// nothing else in the tool says which is which.
#[derive(Default)]
pub struct Settings;

impl Screen for Settings {
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, session: &Session) {
        let palette = session.palette;
        let config = &session.config;
        let thresholds = &config.thresholds;
        let defaults = rqx_core::config::Thresholds::default();
        let source = session.config_file.as_deref().unwrap_or("default");

        let mut rows: Vec<(String, String, bool)> = vec![
            ("rules enabled".to_owned(), format!("{} of {}", session.rules_enabled(), rqx_core::rule::RuleId::ALL.len()), config.rules.disabled.is_empty() && config.rules.enabled.is_empty() && config.rules.categories.is_empty()),
            ("threads".to_owned(), match config.scan.threads { 0 => "auto".to_owned(), n => n.to_string() }, config.scan.threads == 0),
            ("display cap".to_owned(), config.scan.max_findings.to_string(), config.scan.max_findings == 500),
            ("churn window".to_owned(), format!("{} days", config.scan.churn_days), config.scan.churn_days == 90),
            ("gate: fail on".to_owned(), format!("{:?}", config.gate.fail_on).to_lowercase(), matches!(config.gate.fail_on, rqx_core::config::GateSeverity::Error)),
            ("gate: allowed".to_owned(), config.gate.max_findings.to_string(), config.gate.max_findings == 0),
            ("gate: min health".to_owned(), config.gate.min_health.to_string(), config.gate.min_health == 0),
            ("gate: min grade".to_owned(), format!("{:?}", config.gate.min_grade).to_lowercase(), matches!(config.gate.min_grade, rqx_core::config::GateGrade::Never)),
        ];

        for (name, value, default) in [
            ("long function", thresholds.long_function_statements, defaults.long_function_statements),
            ("cognitive complexity", thresholds.cognitive_complexity, defaults.cognitive_complexity),
            ("nesting depth", thresholds.nesting_depth, defaults.nesting_depth),
            ("max parameters", thresholds.max_params, defaults.max_params),
            ("class members", thresholds.class_members, defaults.class_members),
            ("condition operands", thresholds.condition_operands, defaults.condition_operands),
            ("message chain depth", thresholds.message_chain_depth, defaults.message_chain_depth),
            ("magic number repeats", thresholds.magic_number_repeats, defaults.magic_number_repeats),
            ("declaration distance", thresholds.declaration_distance_lines, defaults.declaration_distance_lines),
            ("expression operations", thresholds.expression_operations, defaults.expression_operations),
            ("slow test", thresholds.slow_test_ms, defaults.slow_test_ms),
        ] {
            rows.push((name.to_owned(), value.to_string(), value == default));
        }

        let table: Vec<Row<'static>> = rows
            .into_iter()
            .map(|(name, value, is_default)| {
                Row::new(vec![
                    Span::styled(format!("  {name}"), Style::new().fg(palette.color(Ink::Text))),
                    Span::styled(value, Style::new().fg(palette.color(Ink::Accent))),
                    Span::styled(
                        if is_default { "default".to_owned() } else { source.to_owned() },
                        palette.dim(),
                    ),
                ])
            })
            .collect();

        frame.render_widget(
            Table::new(
                table,
                [Constraint::Length(26), Constraint::Length(14), Constraint::Min(8)],
            )
            .header(Row::new(vec!["  setting", "value", "from"]).style(palette.dim()))
            .block(Block::default().borders(Borders::TOP).border_style(palette.dim())),
            area,
        );
    }

    fn handle(&mut self, key: KeyEvent, _session: &Session) -> Nav {
        global(key).unwrap_or(Nav::Stay)
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[("tab", "next view"), ("esc", "back"), ("q", "quit")]
    }
}

//! Main UI renderer

use once_cell::sync::Lazy;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use regex::Regex;

/// Regex patterns for stripping HTML from markdown
static HTML_TAG_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").unwrap());
static HTML_COMMENT_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"<!--[\s\S]*?-->").unwrap());

/// Strip HTML tags and comments from markdown content
/// GitHub PR descriptions often contain HTML that tui-markdown can't render
fn strip_html(input: &str) -> String {
    let without_comments = HTML_COMMENT_REGEX.replace_all(input, "");
    HTML_TAG_REGEX
        .replace_all(&without_comments, "")
        .to_string()
}

/// Convert markdown string to styled ratatui Text
/// Custom implementation since tui_markdown doesn't render styles correctly
fn markdown_to_text(input: &str) -> Text<'static> {
    // Strip HTML before parsing markdown
    let cleaned = strip_html(input);

    // If input is empty, return empty text
    if cleaned.trim().is_empty() {
        return Text::raw("(no content)");
    }

    let lines: Vec<Line<'static>> = cleaned.lines().map(parse_markdown_line).collect();

    Text::from(lines)
}

/// Parse a single line of markdown into a styled Line
fn parse_markdown_line(line: &str) -> Line<'static> {
    let trimmed = line.trim();

    // Empty line
    if trimmed.is_empty() {
        return Line::from("");
    }

    // Horizontal rule (---, ___, ***)
    if is_horizontal_rule(trimmed) {
        return Line::from(Span::styled(
            "─".repeat(40),
            Style::default().fg(Color::DarkGray),
        ));
    }

    // Headers (# ## ### etc.)
    if let Some((level, content)) = parse_header(trimmed) {
        let style = match level {
            1 => Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            2 => Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            _ => Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        };
        return Line::from(Span::styled(content.to_string(), style));
    }

    // List items (- or * or numbered)
    if let Some(content) = parse_list_item(trimmed) {
        let mut spans = vec![Span::styled("  • ", Style::default().fg(Color::Yellow))];
        spans.extend(parse_inline_spans(content));
        return Line::from(spans);
    }

    // Code block marker (```)
    if trimmed.starts_with("```") {
        let lang = trimmed.trim_start_matches('`').trim();
        if lang.is_empty() {
            return Line::from(Span::styled(
                "───── code ─────",
                Style::default().fg(Color::DarkGray),
            ));
        } else {
            return Line::from(Span::styled(
                format!("───── {} ─────", lang),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    // Blockquote (>)
    if trimmed.starts_with('>') {
        let content = trimmed.trim_start_matches('>').trim();
        return Line::from(vec![
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                content.to_string(),
                Style::default()
                    .fg(Color::Gray)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]);
    }

    // Regular text with inline formatting
    let spans = parse_inline_spans(trimmed);
    Line::from(spans)
}

/// Check if line is a horizontal rule
fn is_horizontal_rule(line: &str) -> bool {
    let chars: Vec<char> = line.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() < 3 {
        return false;
    }
    let first = chars[0];
    (first == '-' || first == '_' || first == '*') && chars.iter().all(|&c| c == first)
}

/// Parse a header line, returns (level, content)
fn parse_header(line: &str) -> Option<(usize, &str)> {
    let mut level = 0;
    let mut chars = line.chars().peekable();

    while chars.peek() == Some(&'#') {
        level += 1;
        chars.next();
    }

    if level == 0 || level > 6 {
        return None;
    }

    // Must have space after #
    if chars.peek() != Some(&' ') {
        return None;
    }

    let content = &line[level..].trim();
    Some((level, content))
}

/// Parse a list item, returns the content without the marker
fn parse_list_item(line: &str) -> Option<&str> {
    // Unordered list (- or *)
    if line.starts_with("- ") || line.starts_with("* ") {
        return Some(&line[2..]);
    }

    // Numbered list (1. 2. etc.)
    let mut chars = line.chars().peekable();
    let mut num_len = 0;
    while chars.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        chars.next();
        num_len += 1;
    }
    if num_len > 0 && chars.next() == Some('.') && chars.next() == Some(' ') {
        return Some(&line[num_len + 2..]);
    }

    None
}

/// Parse inline formatting (bold, italic, code, links)
fn parse_inline_spans(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            // Bold (**text**)
            '*' if chars.peek() == Some(&'*') => {
                if !current.is_empty() {
                    spans.push(Span::raw(std::mem::take(&mut current)));
                }
                chars.next(); // consume second *
                let bold_text = consume_until(&mut chars, "**");
                spans.push(Span::styled(
                    bold_text,
                    Style::default().add_modifier(Modifier::BOLD),
                ));
            }
            // Italic (*text* or _text_)
            '*' | '_' => {
                let delimiter = c;
                if !current.is_empty() {
                    spans.push(Span::raw(std::mem::take(&mut current)));
                }
                let italic_text = consume_until_char(&mut chars, delimiter);
                spans.push(Span::styled(
                    italic_text,
                    Style::default().add_modifier(Modifier::ITALIC),
                ));
            }
            // Inline code (`code`)
            '`' => {
                if !current.is_empty() {
                    spans.push(Span::raw(std::mem::take(&mut current)));
                }
                let code_text = consume_until_char(&mut chars, '`');
                spans.push(Span::styled(
                    code_text,
                    Style::default().fg(Color::Green).bg(Color::Black),
                ));
            }
            // Link [text](url) - just show text
            '[' => {
                if !current.is_empty() {
                    spans.push(Span::raw(std::mem::take(&mut current)));
                }
                let link_text = consume_until_char(&mut chars, ']');
                // Skip the (url) part if present
                if chars.peek() == Some(&'(') {
                    chars.next();
                    consume_until_char(&mut chars, ')');
                }
                spans.push(Span::styled(
                    link_text,
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::UNDERLINED),
                ));
            }
            // Regular character
            _ => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        spans.push(Span::raw(current));
    }

    if spans.is_empty() {
        spans.push(Span::raw(""));
    }

    spans
}

/// Consume characters until we hit the delimiter string
fn consume_until(chars: &mut std::iter::Peekable<std::str::Chars>, delimiter: &str) -> String {
    let mut result = String::new();
    let delim_chars: Vec<char> = delimiter.chars().collect();

    while let Some(&c) = chars.peek() {
        if delim_chars.len() == 2 && c == delim_chars[0] {
            chars.next();
            if chars.peek() == Some(&delim_chars[1]) {
                chars.next();
                break;
            } else {
                result.push(c);
            }
        } else if delim_chars.len() == 1 && c == delim_chars[0] {
            chars.next();
            break;
        } else {
            result.push(c);
            chars.next();
        }
    }
    result
}

/// Consume characters until we hit a single delimiter character
fn consume_until_char(chars: &mut std::iter::Peekable<std::str::Chars>, delimiter: char) -> String {
    let mut result = String::new();
    while let Some(&c) = chars.peek() {
        if c == delimiter {
            chars.next();
            break;
        }
        result.push(c);
        chars.next();
    }
    result
}

use octocrab::models::IssueState;

use crate::github::workflow::{WorkflowConclusion, WorkflowRunStatus};
use crate::tui::app::{App, Screen};
use crate::tui::theme::Theme;

/// Render the UI
pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content
            Constraint::Length(3), // Status bar
        ])
        .split(frame.area());

    render_header(frame, chunks[0], app);
    render_content(frame, chunks[1], app);
    render_status_bar(frame, chunks[2], app);

    // Render help overlay on top if active
    if app.show_help {
        render_help_overlay(frame, app);
    }
}

/// Render the header
fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let repo_name = app
        .repository
        .as_ref()
        .map(|r| r.full_name())
        .unwrap_or_else(|| "No repository".to_string());

    let screen_name = match app.current_screen {
        Screen::Dashboard => "Dashboard",
        Screen::PrList => "Pull Requests",
        Screen::PrDetail(n) => return render_pr_detail_header(frame, area, n),
        Screen::PrCreate => "Create Pull Request",
        Screen::Commit => "Create Commit",
        Screen::Settings => "Settings",
        Screen::Auth => "Authentication",
        Screen::WorkflowRuns => "Workflow Runs",
    };

    let title = format!(" argo-rs │ {} │ {} ", repo_name, screen_name);

    let header = Paragraph::new(title)
        .style(Theme::header())
        .block(Block::default().borders(Borders::BOTTOM));

    frame.render_widget(header, area);
}

fn render_pr_detail_header(frame: &mut Frame, area: Rect, pr_number: u64) {
    let title = format!(" argo-rs │ Pull Request #{} ", pr_number);
    let header = Paragraph::new(title)
        .style(Theme::header())
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(header, area);
}

/// Render the main content area based on current screen
fn render_content(frame: &mut Frame, area: Rect, app: &App) {
    match app.current_screen {
        Screen::Dashboard => render_dashboard(frame, area, app),
        Screen::PrList => render_pr_list(frame, area, app),
        Screen::PrCreate => render_pr_create(frame, area, app),
        Screen::PrDetail(number) => render_pr_detail(frame, area, app, number),
        Screen::Commit => render_commit_screen(frame, area, app),
        Screen::Settings => render_settings(frame, area, app),
        Screen::Auth => render_placeholder(frame, area, "Authentication", "Coming soon..."),
        Screen::WorkflowRuns => render_workflow_runs(frame, area, app),
    }
}

/// Render the dashboard screen
fn render_dashboard(frame: &mut Frame, area: Rect, app: &App) {
    // Split into menu and status
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(area);

    let menu_items = vec![
        ListItem::new("  [p] Pull Requests"),
        ListItem::new("  [n] New Pull Request"),
        ListItem::new("  [c] Create Commit"),
        ListItem::new("  [w] Workflow Runs"),
        ListItem::new("  [s] Settings"),
        ListItem::new("  [q] Quit"),
    ];

    let items: Vec<ListItem> = menu_items
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            if i == app.dashboard_selection.selected {
                item.style(Theme::selected())
            } else {
                item
            }
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Menu ")
                .borders(Borders::ALL)
                .border_style(Theme::normal()),
        )
        .highlight_style(Theme::selected());

    frame.render_widget(list, chunks[0]);

    // Status indicators
    let github_indicator = if app.github_authenticated {
        Span::styled("GitHub ✓", Style::default().fg(Color::Green))
    } else {
        Span::styled(
            "GitHub ✗ (run: gr auth login)",
            Style::default().fg(Color::Red),
        )
    };

    let gemini_indicator = if app.gemini_configured {
        Span::styled("  AI ✓", Style::default().fg(Color::Green))
    } else {
        Span::styled("  AI ✗", Style::default().fg(Color::DarkGray))
    };

    let status_line = Line::from(vec![Span::raw("  "), github_indicator, gemini_indicator]);

    let status = Paragraph::new(status_line);
    frame.render_widget(status, chunks[1]);
}

/// Render the PR list screen
fn render_pr_list(frame: &mut Frame, area: Rect, app: &App) {
    // Help text at the bottom
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    // Determine content based on state
    let items: Vec<ListItem> = if app.pr_list_loading {
        vec![ListItem::new("  Fetching pull requests...")]
    } else if let Some(err) = &app.pr_list_error {
        vec![
            ListItem::new(format!("  Error: {}", err)).style(Style::default().fg(Color::Red)),
            ListItem::new(""),
            ListItem::new("  Press [r] to retry"),
        ]
    } else if !app.pr_list_fetched {
        // Haven't fetched yet - this shouldn't happen normally since we auto-fetch on navigate
        vec![ListItem::new("  Press [r] to load pull requests")]
    } else if app.pr_list.is_empty() {
        vec![
            ListItem::new("  No open pull requests"),
            ListItem::new(""),
            ListItem::new("  Press [n] to create a new PR"),
        ]
    } else {
        app.pr_list
            .iter()
            .enumerate()
            .map(|(i, pr)| {
                let state_icon = if pr.draft == Some(true) {
                    "◇"
                } else {
                    match &pr.state {
                        Some(IssueState::Open) => "○",
                        Some(IssueState::Closed) => "●",
                        None | Some(_) => "○",
                    }
                };

                let title = pr.title.as_deref().unwrap_or("(no title)");
                let author = pr
                    .user
                    .as_ref()
                    .map(|u| u.login.as_str())
                    .unwrap_or("unknown");

                let text = format!("  {} #{} {} ({})", state_icon, pr.number, title, author);
                let item = ListItem::new(text);

                if i == app.pr_list_selection.selected {
                    item.style(Theme::selected())
                } else {
                    item
                }
            })
            .collect()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title(format!(" Pull Requests ({}) ", app.pr_list.len()))
                .borders(Borders::ALL)
                .border_style(Theme::normal()),
        )
        .highlight_style(Theme::selected());

    frame.render_widget(list, chunks[0]);

    let help =
        Paragraph::new(" [n] New PR  [r] Refresh  [Enter] View  [Esc] Back").style(Theme::muted());
    frame.render_widget(help, chunks[1]);
}

/// Render the PR detail screen
fn render_pr_detail(frame: &mut Frame, area: Rect, app: &App, pr_number: u64) {
    // Main vertical layout: content area + help bar
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    // Split content into left panel (PR + comments) and right panel (workflows)
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(main_chunks[0]);

    // Left panel: PR info + comments + optional input
    render_pr_left_panel(frame, content_chunks[0], app, pr_number);

    // Right panel: Workflow runs
    render_pr_workflows_panel(frame, content_chunks[1], app);

    // Help bar
    let help_text = if app.pr_comment_expanded || app.pr_description_expanded {
        " [j/k] Scroll  [Esc/Enter/q] Close"
    } else if app.pr_comment_input_mode {
        " [Enter] Submit  [Esc] Cancel"
    } else {
        " [j/k] Navigate  [Enter] Expand  [d] Description  [c] Comment  [m] Merge  [r] Refresh  [Esc] Back"
    };
    let help = Paragraph::new(help_text).style(Theme::muted());
    frame.render_widget(help, main_chunks[1]);

    // Render expanded comment overlay if active
    if app.pr_comment_expanded {
        render_expanded_comment(frame, app);
    }

    // Render expanded PR description overlay if active
    if app.pr_description_expanded {
        render_expanded_description(frame, app);
    }

    // Render reaction picker overlay if active
    if app.reaction_picker_open {
        render_reaction_picker(frame, app);
    }
}

/// Render the left panel with PR info, description, and comments
fn render_pr_left_panel(frame: &mut Frame, area: Rect, app: &App, pr_number: u64) {
    // Determine layout based on comment input mode
    let constraints = if app.pr_comment_input_mode {
        vec![
            Constraint::Length(5), // PR info (compact)
            Constraint::Length(8), // Description preview
            Constraint::Min(5),    // Comments
            Constraint::Length(3), // Comment input
        ]
    } else {
        vec![
            Constraint::Length(5), // PR info (compact)
            Constraint::Length(8), // Description preview
            Constraint::Min(5),    // Comments
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    // PR Info section (chunks[0])
    if app.pr_detail_loading {
        let loading = Paragraph::new(format!("\n  Loading PR #{}...", pr_number)).block(
            Block::default()
                .title(format!(" PR #{} ", pr_number))
                .borders(Borders::ALL),
        );
        frame.render_widget(loading, chunks[0]);
    } else if let Some(pr) = &app.selected_pr {
        let state_str = match &pr.state {
            Some(IssueState::Open) => "Open",
            Some(IssueState::Closed) => "Closed",
            None | Some(_) => "Unknown",
        };
        let draft_str = if pr.draft == Some(true) {
            " (Draft)"
        } else {
            ""
        };
        let title = pr.title.as_deref().unwrap_or("(no title)");
        let author = pr
            .user
            .as_ref()
            .map(|u| u.login.as_str())
            .unwrap_or("unknown");
        let head_branch = pr.head.ref_field.as_str();
        let base_branch = pr.base.ref_field.as_str();

        let lines: Vec<Line> = vec![
            Line::from(vec![
                Span::styled("Title: ", Style::default().fg(Color::Cyan)),
                Span::raw(truncate(title, 50)),
            ]),
            Line::from(vec![
                Span::styled("State: ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("{}{}", state_str, draft_str),
                    if state_str == "Open" {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Red)
                    },
                ),
                Span::raw("  "),
                Span::styled("Author: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("@{}", author)),
            ]),
            Line::from(vec![
                Span::styled("Branches: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!(
                    "{} → {}",
                    truncate(head_branch, 20),
                    truncate(base_branch, 20)
                )),
            ]),
        ];

        let content = Paragraph::new(lines).block(
            Block::default()
                .title(format!(" PR #{} ", pr_number))
                .borders(Borders::ALL)
                .border_style(Theme::normal()),
        );
        frame.render_widget(content, chunks[0]);
    } else {
        let error = Paragraph::new("\n  Failed to load PR. Press [r] to retry.").block(
            Block::default()
                .title(format!(" PR #{} ", pr_number))
                .borders(Borders::ALL),
        );
        frame.render_widget(error, chunks[0]);
    }

    // PR Description section (chunks[1])
    render_pr_description_preview(frame, chunks[1], app);

    // Comments section (chunks[2])
    render_pr_comments(frame, chunks[2], app);

    // Comment input box (if in input mode) - chunks[3]
    if app.pr_comment_input_mode {
        let input_area = chunks[3];
        let display_text = if app.pr_comment_submitting {
            "Posting comment...".to_string()
        } else {
            format!("{}▌", &app.pr_comment_text)
        };

        let input_style = if app.pr_comment_submitting {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::White)
        };

        let input = Paragraph::new(display_text).style(input_style).block(
            Block::default()
                .title(" New Comment ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );
        frame.render_widget(input, input_area);
    }
}

/// Render the PR description preview in the main view
fn render_pr_description_preview(frame: &mut Frame, area: Rect, app: &App) {
    let pr = match &app.selected_pr {
        Some(pr) => pr,
        None => {
            let empty = Paragraph::new("  Loading...").block(
                Block::default()
                    .title(" Description ")
                    .borders(Borders::ALL),
            );
            frame.render_widget(empty, area);
            return;
        }
    };

    let body = pr.body.as_deref().unwrap_or("(no description)");

    // Use markdown rendering for the description
    let markdown_text = markdown_to_text(body);

    let description = Paragraph::new(markdown_text)
        .block(
            Block::default()
                .title(" Description (d for full) ")
                .borders(Borders::ALL)
                .border_style(Theme::normal()),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(description, area);
}

/// Render the comments section for a PR with selection highlighting
fn render_pr_comments(frame: &mut Frame, area: Rect, app: &App) {
    let title = format!(" Comments ({}) ", app.pr_comments.len());

    let items: Vec<ListItem> = if app.pr_comments_loading {
        vec![ListItem::new("  Loading comments...")]
    } else if let Some(err) = &app.pr_comments_error {
        vec![ListItem::new(format!("  Error: {}", err)).style(Style::default().fg(Color::Red))]
    } else if app.pr_comments.is_empty() {
        vec![ListItem::new("  No comments yet. Press [c] to add one.")]
    } else {
        app.pr_comments
            .iter()
            .enumerate()
            .map(|(i, comment)| {
                let author = &comment.user.login;
                let body_preview = comment
                    .body
                    .as_deref()
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or("");
                let time = format_relative_time(comment.created_at);

                // Get reactions for this comment
                let comment_id: u64 = *comment.id;
                let reactions_str = format_reactions_summary(&app.pr_comment_reactions, comment_id);

                // Build comment text with reactions on same line if any
                let text = if reactions_str.is_empty() {
                    format!("  @{} • {} • {}", author, time, truncate(body_preview, 40))
                } else {
                    format!(
                        "  @{} • {} • {} {}",
                        author,
                        time,
                        truncate(body_preview, 30),
                        reactions_str
                    )
                };

                let item = ListItem::new(text);

                // Highlight selected comment
                if i == app.pr_comments_selection.selected && !app.pr_comments.is_empty() {
                    item.style(Theme::selected())
                } else {
                    item
                }
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Theme::normal()),
    );

    frame.render_widget(list, area);
}

/// Format reactions into a compact summary string like "👍2 ❤️1"
fn format_reactions_summary(
    reactions_map: &std::collections::HashMap<u64, Vec<crate::github::pull_request::Reaction>>,
    comment_id: u64,
) -> String {
    let Some(reactions) = reactions_map.get(&comment_id) else {
        return String::new();
    };

    if reactions.is_empty() {
        return String::new();
    }

    // Count reactions by type
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for reaction in reactions {
        *counts.entry(reaction.emoji()).or_insert(0) += 1;
    }

    // Format as "👍2 ❤️1" etc.
    let mut parts: Vec<String> = Vec::new();
    // Order: thumbs up, thumbs down, heart, hooray (matching picker order)
    for emoji in &["👍", "👎", "❤️", "🎉", "😄", "😕", "🚀", "👀"] {
        if let Some(&count) = counts.get(emoji) {
            parts.push(format!("{}{}", emoji, count));
        }
    }

    parts.join(" ")
}

/// Render the workflow runs panel on the right side
fn render_pr_workflows_panel(frame: &mut Frame, area: Rect, app: &App) {
    let title = format!(" Workflows ({}) ", app.pr_workflow_runs.len());

    let items: Vec<ListItem> = if app.pr_workflow_runs_loading {
        vec![ListItem::new("  Loading...")]
    } else if app.pr_workflow_runs.is_empty() {
        vec![ListItem::new("  No workflow runs")]
    } else {
        app.pr_workflow_runs
            .iter()
            .map(|run| {
                let (icon, icon_color) =
                    workflow_status_display(run.status, run.conclusion, app.tick_counter);
                let text = format!(
                    " {} {} {}",
                    icon,
                    truncate(&run.name, 18),
                    run.duration_string()
                );
                ListItem::new(text).style(Style::default().fg(icon_color))
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Theme::normal()),
    );

    frame.render_widget(list, area);
}

/// Render expanded comment overlay with markdown rendering
fn render_expanded_comment(frame: &mut Frame, app: &App) {
    if app.pr_comments.is_empty() {
        return;
    }

    let comment = &app.pr_comments[app.pr_comments_selection.selected];
    let area = frame.area();

    // Calculate centered popup area (80% width, 70% height)
    let popup_width = (area.width * 80 / 100).max(60);
    let popup_height = (area.height * 70 / 100).max(15);
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Build comment metadata
    let author = &comment.user.login;
    let time = format_relative_time(comment.created_at);
    let body = comment.body.as_deref().unwrap_or("(no content)");

    // Get reactions for this comment
    let comment_id: u64 = *comment.id;
    let reactions_str = format_reactions_summary(&app.pr_comment_reactions, comment_id);

    // Split popup into header, body, and footer
    let inner_area = popup_area.inner(Margin::new(1, 1)); // Account for border
    let header_height = if reactions_str.is_empty() { 2 } else { 3 };
    let footer_height = 2;
    let body_height = inner_area
        .height
        .saturating_sub(header_height + footer_height);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Length(body_height),
            Constraint::Length(footer_height),
        ])
        .split(inner_area);

    // Render the outer block (border)
    let outer_block = Block::default()
        .title(" Comment ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black));
    frame.render_widget(outer_block, popup_area);

    // Render header with author and time
    let mut header_lines: Vec<Line> = vec![Line::from(vec![
        Span::styled("Author: ", Style::default().fg(Color::Cyan)),
        Span::raw(format!("@{}", author)),
        Span::raw("  "),
        Span::styled("Time: ", Style::default().fg(Color::Cyan)),
        Span::raw(time),
    ])];
    if !reactions_str.is_empty() {
        header_lines.push(Line::from(vec![
            Span::styled("Reactions: ", Style::default().fg(Color::Cyan)),
            Span::raw(reactions_str),
        ]));
    }
    header_lines.push(Line::from("─".repeat(chunks[0].width as usize)));

    let header = Paragraph::new(header_lines).style(Style::default().bg(Color::Black));
    frame.render_widget(header, chunks[0]);

    // Render markdown body with scroll support
    let markdown_text = markdown_to_text(body);
    // Estimate wrapped line count (rough: chars / width * 1.5 for wrapping overhead)
    let total_chars: usize = markdown_text.lines.iter().map(|l| l.width()).sum();
    let estimated_lines =
        (total_chars / chunks[1].width.max(1) as usize).max(markdown_text.lines.len()) + 5;
    let visible_height = chunks[1].height as usize;
    let max_scroll = estimated_lines.saturating_sub(visible_height);
    app.pr_comment_max_scroll.set(max_scroll);
    let scroll = app.pr_comment_scroll.min(max_scroll);

    let body_paragraph = Paragraph::new(markdown_text)
        .style(Style::default().bg(Color::Black))
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));
    frame.render_widget(body_paragraph, chunks[1]);

    // Render footer with scroll indicator and actions
    let mut footer_lines: Vec<Line> = Vec::new();
    if max_scroll > 0 {
        footer_lines.push(Line::from(Span::styled(
            format!("[{}/{}] j/k to scroll", scroll + 1, max_scroll + 1),
            Style::default().fg(Color::DarkGray),
        )));
    }
    footer_lines.push(Line::from(Span::styled(
        "[e] Add reaction  [Esc] Close",
        Style::default().fg(Color::DarkGray),
    )));

    let footer = Paragraph::new(footer_lines).style(Style::default().bg(Color::Black));
    frame.render_widget(footer, chunks[2]);
}

/// Render expanded PR description overlay with markdown rendering
fn render_expanded_description(frame: &mut Frame, app: &App) {
    let pr = match &app.selected_pr {
        Some(pr) => pr,
        None => return,
    };

    let area = frame.area();

    // Calculate centered popup area (80% width, 70% height)
    let popup_width = (area.width * 80 / 100).max(60);
    let popup_height = (area.height * 70 / 100).max(15);
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Build PR description metadata
    let title = pr.title.as_deref().unwrap_or("(no title)");
    let author = pr
        .user
        .as_ref()
        .map(|u| u.login.as_str())
        .unwrap_or("unknown");
    let body = pr.body.as_deref().unwrap_or("(no description)");

    // Split popup into header, body, and footer
    let inner_area = popup_area.inner(Margin::new(1, 1)); // Account for border
    let header_height = 3;
    let footer_height = 1;
    let body_height = inner_area
        .height
        .saturating_sub(header_height + footer_height);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Length(body_height),
            Constraint::Length(footer_height),
        ])
        .split(inner_area);

    // Render the outer block (border)
    let outer_block = Block::default()
        .title(" PR Description ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .style(Style::default().bg(Color::Black));
    frame.render_widget(outer_block, popup_area);

    // Render header with title and author
    let header_lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("Title: ", Style::default().fg(Color::Cyan)),
            Span::raw(title),
        ]),
        Line::from(vec![
            Span::styled("Author: ", Style::default().fg(Color::Cyan)),
            Span::raw(format!("@{}", author)),
        ]),
        Line::from("─".repeat(chunks[0].width as usize)),
    ];

    let header = Paragraph::new(header_lines).style(Style::default().bg(Color::Black));
    frame.render_widget(header, chunks[0]);

    // Render markdown body with scroll support
    let markdown_text = markdown_to_text(body);
    // Estimate wrapped line count (rough: chars / width for wrapping)
    let total_chars: usize = markdown_text.lines.iter().map(|l| l.width()).sum();
    let estimated_lines =
        (total_chars / chunks[1].width.max(1) as usize).max(markdown_text.lines.len()) + 5;
    let visible_height = chunks[1].height as usize;
    let max_scroll = estimated_lines.saturating_sub(visible_height);
    app.pr_description_max_scroll.set(max_scroll);
    let scroll = app.pr_description_scroll.min(max_scroll);

    let body_paragraph = Paragraph::new(markdown_text)
        .style(Style::default().bg(Color::Black))
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));
    frame.render_widget(body_paragraph, chunks[1]);

    // Render footer with scroll indicator
    let footer_text = if max_scroll > 0 {
        format!(
            "[{}/{}] j/k to scroll  [Esc] Close",
            scroll + 1,
            max_scroll + 1
        )
    } else {
        "[Esc] Close".to_string()
    };

    let footer = Paragraph::new(Span::styled(
        footer_text,
        Style::default().fg(Color::DarkGray),
    ))
    .style(Style::default().bg(Color::Black));
    frame.render_widget(footer, chunks[2]);
}

/// Render the reaction picker overlay
fn render_reaction_picker(frame: &mut Frame, app: &App) {
    use crate::github::pull_request::ReactionType;

    let area = frame.area();

    // Small centered popup for reaction picker
    let popup_width = 36_u16;
    let popup_height = 5_u16;
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Build reaction options with selection highlighting
    let reactions = ReactionType::all();
    let mut spans: Vec<Span> = Vec::new();

    for (i, reaction) in reactions.iter().enumerate() {
        let label = format!(" [{}] {} ", i + 1, reaction.emoji());
        let style = if i == app.reaction_picker_selection {
            Style::default().bg(Color::Yellow).fg(Color::Black)
        } else {
            Style::default()
        };
        spans.push(Span::styled(label, style));
    }

    let lines = vec![
        Line::from(""),
        Line::from(spans),
        Line::from(""),
        Line::from(Span::styled(
            "  [1-4] Select  [Esc] Cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Add Reaction ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .style(Style::default().bg(Color::Black))
        .alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(paragraph, popup_area);
}

/// Format a datetime as relative time
fn format_relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(dt);

    if duration.num_days() > 30 {
        dt.format("%Y-%m-%d").to_string()
    } else if duration.num_days() > 0 {
        format!("{}d ago", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{}h ago", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{}m ago", duration.num_minutes())
    } else {
        "just now".to_string()
    }
}

/// Render the create PR screen
fn render_pr_create(frame: &mut Frame, area: Rect, app: &App) {
    // Split into form area and help bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    // Form layout: title, branches (side by side), body, draft+submit
    let form_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(8), // Branches (side by side)
            Constraint::Min(5),    // Body
            Constraint::Length(3), // Draft + Submit
        ])
        .split(chunks[0]);

    // Title field (field 0)
    let title_style = if app.pr_create_field == 0 {
        Style::default().fg(Color::Yellow)
    } else {
        Theme::normal()
    };
    let title_text = if app.pr_create_title.is_empty() && app.pr_create_field != 0 {
        Span::styled("Enter PR title...", Style::default().fg(Color::DarkGray))
    } else {
        Span::raw(&app.pr_create_title)
    };
    let title_block = Block::default()
        .title(" Title ")
        .borders(Borders::ALL)
        .border_style(title_style);
    let title_paragraph = Paragraph::new(title_text).block(title_block);
    frame.render_widget(title_paragraph, form_chunks[0]);

    // Branch selectors (side by side)
    let branch_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(form_chunks[1]);

    // Head branch (field 1)
    render_branch_selector(
        frame,
        branch_chunks[0],
        " Head (from) ",
        &app.pr_create_head,
        &app.pr_create_branches,
        app.pr_create_head_selection.selected,
        app.pr_create_field == 1,
        app.pr_create_loading,
    );

    // Base branch (field 2)
    render_branch_selector(
        frame,
        branch_chunks[1],
        " Base (into) ",
        &app.pr_create_base,
        &app.pr_create_branches,
        app.pr_create_base_selection.selected,
        app.pr_create_field == 2,
        app.pr_create_loading,
    );

    // Split body area into description and commits panels
    let body_commits_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(form_chunks[2]);

    // Body/Description field (field 3) - left panel
    let body_style = if app.pr_create_field == 3 {
        Style::default().fg(Color::Yellow)
    } else {
        Theme::normal()
    };
    let body_text = if app.pr_create_body.is_empty() && app.pr_create_field != 3 {
        "Enter PR description (optional)..."
    } else {
        &app.pr_create_body
    };
    let body_block = Block::default()
        .title(" Description ")
        .borders(Borders::ALL)
        .border_style(body_style);
    let body_paragraph = Paragraph::new(body_text).block(body_block).style(
        if app.pr_create_body.is_empty() && app.pr_create_field != 3 {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        },
    );
    frame.render_widget(body_paragraph, body_commits_chunks[0]);

    // Commits list - right panel
    let commits_items: Vec<ListItem> = if app.pr_create_commits.is_empty() {
        vec![ListItem::new("  No commits between branches")
            .style(Style::default().fg(Color::DarkGray))]
    } else {
        app.pr_create_commits
            .iter()
            .map(|c| ListItem::new(format!(" ◇ {}", c)))
            .collect()
    };

    let commits_list = List::new(commits_items).block(
        Block::default()
            .title(format!(" Commits ({}) ", app.pr_create_commits.len()))
            .borders(Borders::ALL)
            .border_style(Theme::normal()),
    );
    frame.render_widget(commits_list, body_commits_chunks[1]);

    // Draft toggle (field 4) and Submit button (field 5)
    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(form_chunks[3]);

    // Draft toggle
    let draft_style = if app.pr_create_field == 4 {
        Style::default().fg(Color::Yellow)
    } else {
        Theme::normal()
    };
    let draft_indicator = if app.pr_create_draft { "[x]" } else { "[ ]" };
    let draft_block = Block::default()
        .title(" Draft ")
        .borders(Borders::ALL)
        .border_style(draft_style);
    let draft_paragraph =
        Paragraph::new(format!(" {} Create as draft PR", draft_indicator)).block(draft_block);
    frame.render_widget(draft_paragraph, bottom_chunks[0]);

    // Submit button
    let submit_style = if app.pr_create_field == 5 {
        Style::default()
            .fg(Color::Green)
            .add_modifier(ratatui::style::Modifier::BOLD)
    } else {
        Theme::normal()
    };
    let submit_text = if app.pr_create_submitting {
        " Creating PR..."
    } else {
        " [ Create Pull Request ]"
    };
    let submit_block = Block::default()
        .borders(Borders::ALL)
        .border_style(submit_style);
    let submit_paragraph = Paragraph::new(submit_text)
        .block(submit_block)
        .alignment(Alignment::Center);
    frame.render_widget(submit_paragraph, bottom_chunks[1]);

    // Show AI loading indicator or error
    if app.pr_create_ai_loading {
        let loading_area = Rect::new(area.x + 2, area.y + area.height - 3, area.width - 4, 1);
        let loading_text =
            Paragraph::new("Generating with AI...").style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading_text, loading_area);
    } else if let Some(error) = &app.pr_create_error {
        let error_area = Rect::new(area.x + 2, area.y + area.height - 3, area.width - 4, 1);
        let error_text =
            Paragraph::new(format!("Error: {}", error)).style(Style::default().fg(Color::Red));
        frame.render_widget(error_text, error_area);
    }

    // Help bar with AI hint if configured
    let help_text = if app.gemini_configured {
        " [Tab] Next  [Enter] Select  [Ctrl+g] AI Generate  [Esc] Cancel"
    } else {
        " [Tab] Next field  [Shift+Tab] Previous  [Enter] Select/Submit  [Esc] Cancel"
    };
    let help = Paragraph::new(help_text).style(Theme::muted());
    frame.render_widget(help, chunks[1]);
}

/// Render a branch selector dropdown
#[allow(clippy::too_many_arguments)]
fn render_branch_selector(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    selected_branch: &str,
    branches: &[crate::github::branch::BranchInfo],
    selection_index: usize,
    is_focused: bool,
    is_loading: bool,
) {
    let style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Theme::normal()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(style);

    if is_loading {
        let loading = Paragraph::new("  Loading branches...")
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(loading, area);
        return;
    }

    if branches.is_empty() {
        let empty = Paragraph::new(format!("  {}", selected_branch)).block(block);
        frame.render_widget(empty, area);
        return;
    }

    // Show selected branch at top, then list if focused
    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    if is_focused {
        // Show scrollable list of branches
        let items: Vec<ListItem> = branches
            .iter()
            .enumerate()
            .map(|(i, branch)| {
                let prefix = if i == selection_index { "› " } else { "  " };
                let suffix = if branch.is_default { " (default)" } else { "" };
                let style = if i == selection_index {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(ratatui::style::Modifier::BOLD)
                } else if branch.name == selected_branch {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{}{}{}", prefix, branch.name, suffix)).style(style)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, inner_area);
    } else {
        // Show just the selected branch
        let text = Paragraph::new(format!("  {}", selected_branch));
        frame.render_widget(text, inner_area);
    }
}

/// Render the commit screen
fn render_commit_screen(frame: &mut Frame, area: Rect, app: &App) {
    // Split into file list, optional message input/push prompt, and help bar
    let constraints = if app.commit_message_mode || app.commit_push_prompt {
        vec![
            Constraint::Min(0),    // File list
            Constraint::Length(3), // Message input box or push prompt
            Constraint::Length(1), // Help bar
        ]
    } else {
        vec![
            Constraint::Min(0),    // File list
            Constraint::Length(1), // Help bar
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    if app.changed_files.is_empty() {
        let text = vec![
            Line::from(""),
            Line::from("  No changes to commit."),
            Line::from(""),
            Line::from("  Your working tree is clean."),
        ];

        let paragraph = Paragraph::new(text).block(
            Block::default()
                .title(" Create Commit ")
                .borders(Borders::ALL)
                .border_style(Theme::normal()),
        );
        frame.render_widget(paragraph, chunks[0]);
    } else {
        // Count staged files
        let staged_count = app.changed_files.iter().filter(|f| f.is_staged).count();

        let items: Vec<ListItem> = app
            .changed_files
            .iter()
            .enumerate()
            .map(|(i, file)| {
                let checkbox = if file.is_staged { "[✓]" } else { "[ ]" };
                let status = file.status_char();
                let text = format!(" {} {} {}", checkbox, status, file.path);
                let item = ListItem::new(text);

                let style = if file.is_staged {
                    Style::default().fg(Color::Green)
                } else if file.is_new {
                    Style::default().fg(Color::Yellow)
                } else if file.is_deleted {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };

                if i == app.commit_file_selection.selected {
                    item.style(Theme::selected())
                } else {
                    item.style(style)
                }
            })
            .collect();

        let title = format!(
            " Create Commit ({}/{} staged) ",
            staged_count,
            app.changed_files.len()
        );

        let list = List::new(items)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Theme::normal()),
            )
            .highlight_style(Theme::selected());

        frame.render_widget(list, chunks[0]);
    }

    // Render message input box if in message mode
    if app.commit_message_mode {
        let message_area = chunks[1];
        let display_text = if app.commit_ai_loading {
            "Generating with AI...".to_string()
        } else {
            format!("{}▌", &app.commit_message) // Show cursor
        };

        let input_style = if app.commit_ai_loading {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::White)
        };

        let input = Paragraph::new(display_text).style(input_style).block(
            Block::default()
                .title(" Commit Message ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );
        frame.render_widget(input, message_area);
    }

    // Render push prompt if showing
    if app.commit_push_prompt {
        let prompt_area = chunks[1];
        let tracking = app.commit_tracking_branch.as_deref().unwrap_or("origin");

        let (display_text, border_color) = if app.commit_push_loading {
            (format!("Pushing to {}...", tracking), Color::Yellow)
        } else {
            let hash = app
                .last_commit_hash
                .as_ref()
                .map(|h| &h[..7.min(h.len())])
                .unwrap_or("commit");
            (
                format!("✓ {} created. Push to {}?", hash, tracking),
                Color::Green,
            )
        };

        let prompt = Paragraph::new(display_text)
            .style(Style::default().fg(if app.commit_push_loading {
                Color::Yellow
            } else {
                Color::Green
            }))
            .block(
                Block::default()
                    .title(" Push to Remote ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color)),
            );
        frame.render_widget(prompt, prompt_area);
    }

    // Help bar (last chunk)
    let help_area = if app.commit_message_mode || app.commit_push_prompt {
        chunks[2]
    } else {
        chunks[1]
    };
    let help_text = if app.commit_push_prompt {
        if app.commit_push_loading {
            "" // No help text during push - status shown in prompt box
        } else {
            " [Enter/y] Push  [Esc/n] Skip"
        }
    } else if app.commit_message_mode {
        " [Enter] Commit  [Esc] Cancel  [Ctrl+g] Regenerate AI"
    } else {
        " [Space] Toggle  [a] Stage all  [r] Refresh  [Enter] Commit  [g] AI  [Esc] Back"
    };
    let help = Paragraph::new(help_text).style(Theme::muted());
    frame.render_widget(help, help_area);
}

/// Render the settings screen
fn render_settings(frame: &mut Frame, area: Rect, app: &App) {
    // Split into main content and help bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let (github_text, github_color) = if app.github_authenticated {
        ("Authenticated ✓", Color::Green)
    } else {
        ("Not authenticated ✗", Color::Red)
    };

    let sel = app.settings_selection.selected;

    // GitHub line
    let github_line = Line::from(vec![
        Span::raw(if sel == 0 { " ▶ " } else { "   " }),
        Span::styled("GitHub:      ", Style::default().fg(Color::Cyan)),
        Span::styled(github_text, Style::default().fg(github_color)),
    ]);

    // Gemini API key line - show input field when editing
    let gemini_line = if app.settings_input_mode && sel == 1 {
        // Input mode: show masked input with cursor
        let masked_input = "•".repeat(app.settings_api_key_input.len());
        Line::from(vec![
            Span::raw(" ▶ "),
            Span::styled("Gemini API:  ", Style::default().fg(Color::Cyan)),
            Span::styled("[", Style::default().fg(Color::Yellow)),
            Span::styled(masked_input, Style::default().fg(Color::White)),
            Span::styled("█", Style::default().fg(Color::Yellow)), // cursor
            Span::styled("]", Style::default().fg(Color::Yellow)),
        ])
    } else {
        let (gemini_text, gemini_color) = if app.gemini_configured {
            ("Configured ✓", Color::Green)
        } else {
            ("Not configured ✗", Color::Yellow)
        };
        Line::from(vec![
            Span::raw(if sel == 1 { " ▶ " } else { "   " }),
            Span::styled("Gemini API:  ", Style::default().fg(Color::Cyan)),
            Span::styled(gemini_text, Style::default().fg(gemini_color)),
        ])
    };

    // Model line - show current model from app state
    let model_line = Line::from(vec![
        Span::raw(if sel == 2 { " ▶ " } else { "   " }),
        Span::styled("AI Model:    ", Style::default().fg(Color::Cyan)),
        Span::styled(
            app.gemini_model.display_name(),
            Style::default().fg(Color::White),
        ),
        Span::styled(" (j/k to cycle)", Style::default().fg(Color::DarkGray)),
    ]);

    // Build help text based on current selection and mode
    let help_section = if app.settings_input_mode {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Entering API Key (hidden):",
                Style::default().fg(Color::Yellow),
            )),
            Line::from(""),
            Line::from("  Type your Gemini API key, then press Enter to save"),
            Line::from("  Press Esc to cancel"),
        ]
    } else {
        let help_text = match sel {
            0 => {
                if app.github_authenticated {
                    "  Run: gr auth logout   (to sign out)"
                } else {
                    "  Run: gr auth login    (to authenticate)"
                }
            }
            1 => "  Press Enter to configure API key",
            2 => "  Press j/k or Enter to cycle through models",
            _ => "",
        };
        vec![
            Line::from(""),
            Line::from(Span::styled("  Actions:", Style::default().fg(Color::Cyan))),
            Line::from(""),
            Line::from(help_text),
        ]
    };

    let mut all_lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Authentication & API Keys",
            Style::default().add_modifier(ratatui::style::Modifier::BOLD),
        )),
        Line::from(""),
        github_line,
        gemini_line,
        model_line,
    ];
    all_lines.extend(help_section);

    let paragraph = Paragraph::new(all_lines).block(
        Block::default()
            .title(" Settings ")
            .borders(Borders::ALL)
            .border_style(Theme::normal()),
    );

    frame.render_widget(paragraph, chunks[0]);

    // Update bottom help bar based on mode
    let help_bar = if app.settings_input_mode {
        " [Enter] Save  [Esc] Cancel"
    } else {
        " [j/k] Navigate  [Enter] Edit  [Esc] Back"
    };
    let help = Paragraph::new(help_bar).style(Theme::muted());
    frame.render_widget(help, chunks[1]);
}

/// Get status display icon and color for a workflow run
fn workflow_status_display(
    status: WorkflowRunStatus,
    conclusion: Option<WorkflowConclusion>,
    tick_counter: u64,
) -> (&'static str, Color) {
    const SPINNER: &[&str] = &["\u{25d0}", "\u{25d3}", "\u{25d1}", "\u{25d2}"]; // ◐ ◓ ◑ ◒

    if status.is_active() {
        let frame = SPINNER[tick_counter as usize % SPINNER.len()];
        (frame, Color::Yellow)
    } else {
        match conclusion {
            Some(WorkflowConclusion::Success) => ("\u{2713}", Color::Green), // ✓
            Some(WorkflowConclusion::Failure) => ("\u{2717}", Color::Red),   // ✗
            Some(WorkflowConclusion::Cancelled) => ("\u{25cb}", Color::Gray), // ○
            Some(WorkflowConclusion::Skipped) => ("\u{2298}", Color::Gray),  // ⊘
            Some(WorkflowConclusion::TimedOut) => ("\u{23f1}", Color::Red),  // ⏱
            Some(WorkflowConclusion::ActionRequired) => ("!", Color::Yellow),
            _ => ("?", Color::Gray),
        }
    }
}

/// Truncate a string to max length with ellipsis
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Render the workflow runs screen
fn render_workflow_runs(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let items: Vec<ListItem> = if app.workflow_runs_loading {
        vec![ListItem::new("  Loading workflow runs...")]
    } else if let Some(err) = &app.workflow_runs_error {
        vec![
            ListItem::new(format!("  Error: {}", err)).style(Style::default().fg(Color::Red)),
            ListItem::new(""),
            ListItem::new("  Press [r] to retry"),
        ]
    } else if !app.workflow_runs_fetched {
        vec![ListItem::new("  Press [r] to load workflow runs")]
    } else if app.workflow_runs.is_empty() {
        vec![ListItem::new("  No workflow runs found")]
    } else {
        app.workflow_runs
            .iter()
            .enumerate()
            .map(|(i, run)| {
                let (icon, icon_color) =
                    workflow_status_display(run.status, run.conclusion, app.tick_counter);

                let text = format!(
                    "  {} #{:<4} {:<22} {:<10} {} {:<12} {}",
                    icon,
                    run.run_number,
                    truncate(&run.name, 22),
                    truncate(&run.head_branch, 10),
                    run.head_sha_short,
                    run.event,
                    run.duration_string(),
                );

                let item = ListItem::new(text);

                if i == app.workflow_runs_selection.selected {
                    item.style(Theme::selected())
                } else {
                    item.style(Style::default().fg(icon_color))
                }
            })
            .collect()
    };

    let title = if let Some(ref branch) = app.pr_workflow_branch {
        if app.workflow_runs.is_empty() {
            format!(" Workflow Runs (branch: {}) ", branch)
        } else {
            format!(
                " Workflow Runs ({}) - branch: {} ",
                app.workflow_runs.len(),
                branch
            )
        }
    } else if app.workflow_runs.is_empty() {
        " Workflow Runs ".to_string()
    } else {
        format!(" Workflow Runs ({}) ", app.workflow_runs.len())
    };

    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Theme::normal()),
    );

    frame.render_widget(list, chunks[0]);

    let help = Paragraph::new(" [r] Refresh  [j/k] Navigate  [Esc] Back").style(Theme::muted());
    frame.render_widget(help, chunks[1]);
}

/// Render a placeholder screen
fn render_placeholder(frame: &mut Frame, area: Rect, title: &str, message: &str) {
    let paragraph = Paragraph::new(format!("\n  {}", message)).block(
        Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::ALL),
    );
    frame.render_widget(paragraph, area);
}

/// Render the status bar
fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let branch = app
        .repository
        .as_ref()
        .map(|r| r.current_branch.as_str())
        .unwrap_or("N/A");

    let status_text = if let Some(msg) = &app.status_message {
        msg.clone()
    } else {
        format!(" Branch: {} │ ? for help ", branch)
    };

    let status = Paragraph::new(status_text)
        .style(Theme::status_bar())
        .block(Block::default().borders(Borders::TOP));

    frame.render_widget(status, area);
}

/// Render the help overlay
fn render_help_overlay(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Calculate centered popup area (60% width, 70% height)
    let popup_width = (area.width * 60 / 100).min(60);
    let popup_height = (area.height * 70 / 100).min(20);
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Build help text based on current screen
    let (title, help_lines) = get_help_content(app.current_screen);

    let text: Vec<Line> = help_lines
        .into_iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(format!("  {:12}", key), Style::default().fg(Color::Cyan)),
                Span::raw(desc),
            ])
        })
        .collect();

    let help = Paragraph::new(text)
        .block(
            Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .style(Style::default().bg(Color::Black));

    frame.render_widget(help, popup_area);
}

/// Get help content for the current screen
fn get_help_content(screen: Screen) -> (&'static str, Vec<(&'static str, &'static str)>) {
    let global_keys = vec![
        ("?", "Show this help"),
        ("q / Esc", "Go back / Quit"),
        ("j / ↓", "Move down"),
        ("k / ↑", "Move up"),
        ("Enter", "Select / Confirm"),
    ];

    match screen {
        Screen::Dashboard => (
            "Help - Dashboard",
            vec![
                ("p", "Go to Pull Requests"),
                ("n", "Create new Pull Request"),
                ("c", "Create Commit"),
                ("w", "Workflow Runs"),
                ("s", "Settings"),
                ("q", "Quit application"),
                ("?", "Show this help"),
            ],
        ),
        Screen::PrList => (
            "Help - Pull Requests",
            vec![
                ("j / ↓", "Move down"),
                ("k / ↑", "Move up"),
                ("Enter", "View PR details"),
                ("n", "Create new PR"),
                ("r", "Refresh list"),
                ("Esc", "Go back"),
                ("?", "Show this help"),
            ],
        ),
        Screen::PrDetail(_) => (
            "Help - PR Detail",
            vec![
                ("j / ↓", "Scroll down"),
                ("k / ↑", "Scroll up"),
                ("c", "Add comment"),
                ("w", "View workflows"),
                ("m", "Merge PR"),
                ("r", "Refresh"),
                ("Esc", "Go back"),
                ("?", "Show this help"),
            ],
        ),
        Screen::Settings => (
            "Help - Settings",
            vec![
                ("j / ↓", "Move down"),
                ("k / ↑", "Move up"),
                ("Enter", "Edit setting"),
                ("Esc", "Go back"),
                ("?", "Show this help"),
            ],
        ),
        Screen::Commit => (
            "Help - Commit",
            vec![
                ("Space", "Toggle file staging"),
                ("g", "Generate AI message"),
                ("Enter", "Commit changes"),
                ("Esc", "Cancel / Go back"),
                ("?", "Show this help"),
            ],
        ),
        Screen::PrCreate => (
            "Help - Create PR",
            vec![
                ("Tab", "Next field"),
                ("Shift+Tab", "Previous field"),
                ("g", "Generate AI title/body"),
                ("Enter", "Create PR"),
                ("Esc", "Cancel"),
                ("?", "Show this help"),
            ],
        ),
        Screen::Auth => ("Help - Authentication", global_keys),
        Screen::WorkflowRuns => (
            "Help - Workflow Runs",
            vec![
                ("j / ↓", "Move down"),
                ("k / ↑", "Move up"),
                ("r", "Refresh"),
                ("Esc", "Go back"),
                ("?", "Show this help"),
            ],
        ),
    }
}

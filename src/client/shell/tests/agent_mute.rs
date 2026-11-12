//! `ui.muted_agents`: muted completions stay quiet, blocked still notifies.

use super::*;

fn muted_config(entries: &[&str]) -> ClientShellConfig {
    let mut source = Config::default();
    source.ui.muted_agents = entries.iter().map(|entry| (*entry).to_owned()).collect();
    let mut config = ClientShellConfig::from_config(&source);
    config.toast_delivery = crate::config::ToastDelivery::Terminal;
    config.toast_delay_seconds = 0;
    config
}

fn named_agent(pane_id: &str, name: &str, status: AgentStatus, seq: u64) -> ClientShellAgent {
    ClientShellAgent {
        pane_id: pane_id.into(),
        workspace_id: "ws_1".into(),
        tab_id: "tab_1".into(),
        name: Some(name.into()),
        display_agent: None,
        agent: Some("pi".into()),
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: status,
        state_change_seq: seq,
        state_labels: Vec::new(),
        tokens: Vec::new(),
        focused: false,
    }
}

/// Drives `cron-backup` (muted) and `reviewer` (not muted) from working to a
/// client-observed completion, so the unmuted one projects as `Done`.
fn completed_state(entries: &[&str]) -> ClientShellState {
    let mut state = ClientShellState::new(muted_config(entries));
    let mut projected = snapshot();
    projected.agents = vec![
        named_agent("pane_1", "cron-backup", AgentStatus::Working, 1),
        named_agent("pane_2", "reviewer", AgentStatus::Working, 1),
    ];
    state.set_snapshot(Box::new(projected.clone()));
    projected.revision += 1;
    for agent in &mut projected.agents {
        agent.agent_status = AgentStatus::Idle;
        agent.state_change_seq = 2;
    }
    state.set_snapshot(Box::new(projected));
    state
}

fn status_of(state: &ClientShellState, pane_id: &str) -> AgentStatus {
    state
        .snapshot
        .as_deref()
        .and_then(|snapshot| {
            snapshot
                .agents
                .iter()
                .find(|agent| agent.pane_id == pane_id)
        })
        .map(|agent| agent.agent_status)
        .expect("projected agent")
}

fn finished(pane_id: &str) -> SemanticNotification {
    SemanticNotification {
        kind: SemanticNotificationKind::Finished,
        title: format!("{pane_id} finished"),
        body: None,
        sound: Some(SemanticNotificationSound::Done),
        agent: Some("pi".into()),
        workspace_id: Some("ws_1".into()),
        tab_id: Some("background-tab".into()),
        pane_id: Some(pane_id.into()),
        position: None,
    }
}

#[test]
fn muted_completion_projects_idle_and_never_notifies_or_sounds() {
    let mut state = completed_state(&["re:^cron-"]);
    assert_eq!(status_of(&state, "pane_1"), AgentStatus::Idle);
    assert_eq!(status_of(&state, "pane_2"), AgentStatus::Done);
    // The muted agent no longer lights up its workspace; the unmuted one does.
    assert_eq!(
        state.snapshot.as_deref().unwrap().workspaces[0].agent_status,
        AgentStatus::Done
    );

    let now = std::time::Instant::now();
    let (effects, _) =
        state.receive_notification(&ClientEndpointId::Local, finished("pane_1"), now);
    assert!(effects.is_empty(), "muted completion produced effects");
    assert!(state.pending_notifications.is_empty());

    let (effects, _) =
        state.receive_notification(&ClientEndpointId::Local, finished("pane_2"), now);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ClientShellNotificationEffect::Sound { .. })),
        "expected a sound effect"
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        ClientShellNotificationEffect::Terminal { title, .. } if title == "pane_2 finished"
    )));
}

#[test]
fn muting_only_the_done_agent_clears_workspace_done_highlight() {
    let state = completed_state(&["cron-backup", "reviewer"]);
    assert_eq!(status_of(&state, "pane_1"), AgentStatus::Idle);
    assert_eq!(status_of(&state, "pane_2"), AgentStatus::Idle);
    assert_eq!(
        state.snapshot.as_deref().unwrap().workspaces[0].agent_status,
        AgentStatus::Idle
    );
}

#[test]
fn muted_blocked_agent_still_notifies_and_is_marked() {
    let mut state = ClientShellState::new(muted_config(&["cron-backup"]));
    let mut projected = snapshot();
    projected.agents = vec![named_agent(
        "pane_1",
        "cron-backup",
        AgentStatus::Blocked,
        1,
    )];
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    assert_eq!(status_of(&state, "pane_1"), AgentStatus::Blocked);

    let (effects, _) = state.receive_notification(
        &ClientEndpointId::Local,
        SemanticNotification {
            kind: SemanticNotificationKind::NeedsAttention,
            title: "cron-backup needs attention".into(),
            body: None,
            sound: Some(SemanticNotificationSound::Request),
            agent: Some("pi".into()),
            workspace_id: Some("ws_1".into()),
            tab_id: Some("background-tab".into()),
            pane_id: Some("pane_1".into()),
            position: None,
        },
        std::time::Instant::now(),
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ClientShellNotificationEffect::Sound { .. })),
        "expected a sound effect"
    );

    state.mode = ClientShellMode::Navigate;
    let frame = state.compose(44, 30).expect("mobile switcher");
    let text = frame
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(text.contains("attention"), "{text}");
    assert!(text.contains("⊘ muted"), "{text}");
    assert!(state.hits.mobile_targets.iter().any(|(_, target)| matches!(
        target,
        ClientMobileTarget::Agent { pane_id, .. } if pane_id == "pane_1"
    )));
}

#[test]
fn muted_done_agent_is_absent_from_mobile_attention() {
    let mut state = completed_state(&["cron-backup"]);
    let mut presented = surface();
    presented.projection_revision = 2;
    state.set_pane_surface(presented);
    state.mode = ClientShellMode::Navigate;
    state.compose(44, 30).expect("mobile switcher");
    let agents = state
        .hits
        .mobile_targets
        .iter()
        .filter_map(|(_, target)| match target {
            ClientMobileTarget::Agent { pane_id, .. } => Some(pane_id.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(agents, ["pane_2"]);
}

#[test]
fn desktop_agent_rows_mark_muted_agents() {
    let mut state = completed_state(&["cron-backup"]);
    let mut presented = surface();
    presented.projection_revision = 2;
    state.set_pane_surface(presented);
    let frame = state.compose(120, 30).expect("desktop shell");
    let lines = frame
        .cells
        .chunks(frame.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let marked = lines.iter().filter(|line| line.contains('⊘')).count();
    assert_eq!(marked, 1, "{}", lines.join("\n"));
}

#[test]
fn reloading_rules_reprojects_cached_completions() {
    let mut state = completed_state(&[]);
    assert_eq!(status_of(&state, "pane_1"), AgentStatus::Done);

    state.config.muted_agents = crate::config::AgentMuteRules::compile(&["cron-backup".to_owned()]);
    state.reproject_agent_presentation();
    assert_eq!(status_of(&state, "pane_1"), AgentStatus::Idle);
    assert_eq!(status_of(&state, "pane_2"), AgentStatus::Done);

    // Unmuting restores the unseen completion instead of losing it.
    state.config.muted_agents = crate::config::AgentMuteRules::default();
    state.reproject_agent_presentation();
    assert_eq!(status_of(&state, "pane_1"), AgentStatus::Done);
}

#[test]
fn muted_agents_config_parses_and_reports_bad_patterns() {
    let config: Config = toml::from_str("[ui]\nmuted_agents = [\"daily\", \"re:(\"]\n").unwrap();
    assert_eq!(config.ui.muted_agents, ["daily", "re:("]);
    let diagnostics = config.collect_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("ui.muted_agents")),
        "{diagnostics:?}"
    );
}

#[test]
fn navigator_marks_muted_agents() {
    let mut state = completed_state(&["re:^cron-"]);
    let mut presented = surface();
    presented.projection_revision = 2;
    state.set_pane_surface(presented);
    state.open_navigator_overlay();
    let frame = state.compose(120, 40).expect("navigator");
    let marked = frame
        .cells
        .chunks(frame.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .filter(|line| line.contains('⊘'))
        .collect::<Vec<_>>();
    // One row in the navigator plus the desktop agent row behind it may be
    // occluded, so only require the navigator row for the muted pane.
    assert!(
        marked.iter().any(|line| line.contains("cron-backup")),
        "{marked:?}"
    );
    assert!(
        !marked.iter().any(|line| line.contains("reviewer")),
        "{marked:?}"
    );
}

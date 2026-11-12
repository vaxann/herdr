use super::*;

#[test]
fn navigate_update_status_uses_released_desktop_and_mobile_placement() {
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.tab_bar_position = crate::config::TabBarPositionConfig::Bottom;
    config.hide_tab_bar_when_single_tab = false;
    let mut state = ClientShellState::new(config);
    let mut endpoint_snapshot = snapshot();
    endpoint_snapshot.update_available = Some("0.8.3".into());
    state.set_snapshot(Box::new(endpoint_snapshot));
    state.set_pane_surface(surface());
    state.mode = ClientShellMode::Navigate;

    let bottom = state.compose(106, 30).expect("bottom-tab update shell");
    let row_text = |frame: &FrameData, row: u16| {
        let width = usize::from(frame.width);
        let start = usize::from(row) * width;
        frame.cells[start..start + width]
            .iter()
            .map(|cell| cell.symbol.as_str())
            .collect::<String>()
    };
    assert!(row_text(&bottom, 29).contains("update ready"));
    assert!(!row_text(&bottom, 28).contains("update ready"));
    assert!(state.hits.tabs.is_empty());
    assert!(state.hits.new_tab.is_empty());
    assert!(state.hits.tab_scroll_left.is_empty());
    assert!(state.hits.tab_scroll_right.is_empty());

    state.config.tab_bar_position = crate::config::TabBarPositionConfig::Top;
    state.visible_notification = Some(ClientVisibleNotification {
        endpoint_id: ClientEndpointId::Local,
        event: SemanticNotification {
            kind: SemanticNotificationKind::Custom,
            title: "bottom notification".into(),
            body: None,
            sound: None,
            agent: None,
            workspace_id: None,
            tab_id: None,
            pane_id: None,
            position: Some(crate::config::ToastHerdrPosition::BottomRight),
        },
        deadline: std::time::Instant::now(),
    });
    let top = state.compose(106, 30).expect("top-tab update shell");
    assert!(row_text(&top, 29).contains("update ready"));

    let mobile = state.compose(44, 30).expect("mobile update shell");
    let mobile_text = mobile
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(mobile_text.contains("update ready"));
}

#[test]
fn mobile_layout_reserves_only_client_header() {
    let config = ClientShellConfig::from_config(&Config::default());
    let state = ClientShellState::new(config);
    let layout = state.layout(44, 20);
    assert_eq!(layout.mobile_header, Rect::new(0, 0, 44, 2));
    assert_eq!(layout.pane_surface, Rect::new(0, 2, 44, 18));
    assert_eq!(
        state.surface_size(44, 20),
        ClientSurfaceSize { cols: 44, rows: 18 }
    );
}

#[test]
fn mobile_switcher_can_activate_an_online_saved_machine() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    let profile = crate::client::endpoint::SavedSshEndpoint {
        id: crate::client::endpoint::ProfileId::parse("0123456789abcdef0123456789abcdef").unwrap(),
        label: "Build".into(),
        target: "build".into(),
        session: "agents".into(),
        enabled: true,
    };
    let endpoint_id = ClientEndpointId::Ssh(profile.id.clone());
    state.set_endpoint_catalog(&[profile]);
    state.set_endpoint_status(&endpoint_id, ClientEndpointStatus::Online);
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    let mut remote = snapshot();
    remote.boot_id = "remote-boot".into();
    state.set_endpoint_snapshot(&endpoint_id, Box::new(remote));
    state.mode = ClientShellMode::Navigate;

    let frame = state.compose(44, 30).expect("mobile machine switcher");
    let text = frame
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(text.contains("spaces"));
    assert!(text.contains("Build · online"), "frame: {text}");
    assert!(!text.contains("machines"), "frame: {text}");
    let machine = state
        .hits
        .mobile_targets
        .iter()
        .find_map(|(rect, target)| {
            matches!(target, ClientMobileTarget::Machine(id) if id == &endpoint_id).then_some(*rect)
        })
        .expect("saved machine target");
    let outcome = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: machine.x,
        row: machine.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(matches!(
        outcome.actions.as_slice(),
        [ClientShellAction::ActivateEndpoint {
            endpoint_id: activated,
            target: None,
        }] if activated == &endpoint_id
    ));

    // Local is still projected until the remote handoff completes.
    state.mode = ClientShellMode::Navigate;
    state.compose(44, 30).unwrap();
    let local = state
        .hits
        .mobile_targets
        .iter()
        .find_map(|(rect, target)| {
            matches!(target, ClientMobileTarget::Machine(ClientEndpointId::Local)).then_some(*rect)
        })
        .unwrap();
    let outcome = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: local.x,
        row: local.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(matches!(
        outcome.actions.as_slice(),
        [ClientShellAction::ActivateEndpoint {
            endpoint_id: ClientEndpointId::Local,
            target: None,
        }]
    ));

    state.set_endpoint_status(&endpoint_id, ClientEndpointStatus::Online);
    assert!(state.activate_endpoint_projection(&endpoint_id));
    let mut remote_surface = surface();
    remote_surface.boot_id = "remote-boot".into();
    state.set_pane_surface(remote_surface);
    state.mark_endpoint_disconnected(&endpoint_id);
    state.receive_endpoint_unavailable("network lost".into());
    state.mode = ClientShellMode::Navigate;
    let stale = state.compose(44, 30).expect("stale mobile switcher");
    let text = stale
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(text.contains("reconnecting"), "frame: {text}");
    assert!(text.contains("network lost"), "frame: {text}");
}

#[test]
fn mobile_shell_controls_remain_clickable_when_pane_mouse_capture_is_disabled() {
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.mouse_capture = false;
    let mut state = ClientShellState::new(config);
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state.compose(44, 20).expect("mobile header");
    assert!(!state.hits.mobile_switch.is_empty());
    let switch = state.hits.mobile_switch;
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: switch.x,
        row: switch.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert_eq!(state.mode, ClientShellMode::Navigate);
    state.compose(44, 20).expect("mobile switcher");
    assert!(!state.hits.mobile_close.is_empty());
    assert!(!state.hits.mobile_targets.is_empty());
}

#[test]
fn mobile_header_and_switcher_render_released_sections_and_stable_targets() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    let mut projected = snapshot();
    projected.agents.push(ClientShellAgent {
        pane_id: "pane_1".into(),
        workspace_id: "ws_1".into(),
        tab_id: "tab_1".into(),
        name: Some("pi".into()),
        display_agent: Some("pi".into()),
        agent: Some("pi".into()),
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: AgentStatus::Blocked,
        state_change_seq: 1,
        state_labels: vec![("blocked".into(), "waiting".into())],
        tokens: Vec::new(),
        focused: true,
    });
    projected.workspaces[0].agent_status = AgentStatus::Blocked;
    state.set_snapshot(Box::new(projected));
    let mut projected_surface = surface();
    for cell in &mut projected_surface.frame.cells {
        cell.symbol = "X".to_owned();
    }
    state.set_pane_surface(projected_surface);

    let header = state.compose(44, 20).expect("mobile header");
    let header_text = header
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(header_text.contains("client-shell"));
    assert!(header_text.contains("tab 1"));
    assert!(header_text.contains("blocked"));
    assert!(header_text.contains("switch"));
    assert_eq!(state.hits.mobile_switch, Rect::new(34, 0, 10, 2));

    let click = |rect: Rect| {
        RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::empty(),
        })
    };
    let opened = state.handle_raw_events(vec![click(state.hits.mobile_switch)]);
    assert!(opened.repaint);
    assert_eq!(state.mode, ClientShellMode::Navigate);
    let switcher = state.compose(44, 20).expect("mobile switcher");
    let switcher_text = switcher
        .cells
        .chunks(switcher.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !switcher_text.contains('X'),
        "switcher must clear the pane surface"
    );
    for expected in [
        "switch",
        "close",
        "attention",
        "spaces",
        "+ new workspace",
        "tabs",
        "+ new tab",
        "menu",
        "settings",
        "detach",
    ] {
        assert!(switcher_text.contains(expected), "missing {expected}");
    }
    let workspace_hit = state
        .hits
        .mobile_targets
        .iter()
        .find_map(|(rect, target)| {
            matches!(
                target,
                ClientMobileTarget::Workspace { workspace_id, .. } if workspace_id == "ws_1"
            )
            .then_some(*rect)
        })
        .expect("workspace hit");
    let focused = state.handle_raw_events(vec![click(workspace_hit)]);
    assert_eq!(state.mode, ClientShellMode::Terminal);
    assert!(state.navigate_workspace_id.is_none());
    assert!(focused.actions.iter().any(|action| matches!(
        action,
        ClientShellAction::Endpoint { request, .. }
            if matches!(
                &request.method,
                crate::api::schema::Method::WorkspaceFocus(params)
                    if params.workspace_id == "ws_1"
            )
    )));

    state.compose(44, 20).expect("restored mobile header");
    state.handle_raw_events(vec![click(state.hits.mobile_switch)]);
    state.compose(44, 20).expect("agent switcher");
    let agent_hit = state
        .hits
        .mobile_targets
        .iter()
        .find_map(|(rect, target)| {
            matches!(
                target,
                ClientMobileTarget::Agent { pane_id, .. } if pane_id == "pane_1"
            )
            .then_some(*rect)
        })
        .expect("agent hit");
    let focused = state.handle_raw_events(vec![click(agent_hit)]);
    assert!(focused.actions.iter().any(|action| matches!(
        action,
        ClientShellAction::Endpoint { request, .. }
            if matches!(
                &request.method,
                crate::api::schema::Method::PaneFocus(params)
                    if params.pane_id == "pane_1"
            )
    )));

    state.compose(44, 20).expect("restored mobile header");
    state.handle_raw_events(vec![click(state.hits.mobile_switch)]);
    state.compose(44, 20).expect("tab switcher");
    let tab_hit = state
        .hits
        .mobile_targets
        .iter()
        .find_map(|(rect, target)| {
            matches!(
                target,
                ClientMobileTarget::Tab {
                    endpoint_id,
                    tab_id,
                } if endpoint_id.is_local() && tab_id == "tab_1"
            )
            .then_some(*rect)
        })
        .expect("tab hit");
    let focused = state.handle_raw_events(vec![click(tab_hit)]);
    assert!(focused.actions.iter().any(|action| matches!(
        action,
        ClientShellAction::Endpoint { request, .. }
            if matches!(
                &request.method,
                crate::api::schema::Method::TabFocus(params)
                    if params.tab_id == "tab_1"
            )
    )));
}

#[test]
fn mobile_background_workspace_uses_its_own_active_tab_status() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    let mut projected = snapshot();
    projected.tabs.push(ClientShellTab {
        tab_id: "tab_7".into(),
        workspace_id: "ws_1".into(),
        number: 7,
        label: "logs".into(),
        custom_label: true,
        zoomed: false,
        focused: false,
        agent_status: AgentStatus::Idle,
    });
    projected.workspaces.push(ClientShellWorkspace {
        workspace_id: "ws_2".into(),
        active_tab_id: "tab_3".into(),
        new_workspace_cwd: "/feature".into(),
        number: 2,
        label: "background".into(),
        custom_label: true,
        branch: Some("feature".into()),
        git_ahead_behind: None,
        tokens: Vec::new(),
        worktree: None,
        focused: false,
        agent_status: AgentStatus::Idle,
    });
    for (number, tab_id, label) in [(1, "tab_2", "one"), (7, "tab_3", "two")] {
        projected.tabs.push(ClientShellTab {
            tab_id: tab_id.into(),
            workspace_id: "ws_2".into(),
            number,
            label: label.into(),
            custom_label: true,
            zoomed: false,
            focused: false,
            agent_status: AgentStatus::Idle,
        });
    }
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state.mode = ClientShellMode::Navigate;
    state.navigate_workspace_id = state.navigation_target(&ClientEndpointId::Local, "ws_2");
    let frame = state.compose(44, 20).expect("mobile switcher");
    let text = frame
        .cells
        .chunks(frame.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("feature · tab two · 2/2"), "{text}");
    assert!(text.contains("2 · logs"), "{text}");
    assert!(!text.contains("7 · logs"), "{text}");
}

#[test]
fn mobile_switcher_create_and_menu_rows_reuse_client_actions() {
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.prompt_new_workspace_name = true;
    config.prompt_new_tab_name = true;
    let mut state = ClientShellState::new(config);
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    let click = |rect: Rect| {
        RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::empty(),
        })
    };

    state.mode = ClientShellMode::Navigate;
    state.compose(44, 20).expect("mobile create switcher");
    let new_tab = state
        .hits
        .mobile_targets
        .iter()
        .find_map(|(rect, target)| matches!(target, ClientMobileTarget::NewTab).then_some(*rect))
        .expect("new tab hit");
    state.handle_raw_events(vec![click(new_tab)]);
    assert!(matches!(
        state.overlay,
        Some(ClientShellOverlay::Rename(ClientRenameOverlay {
            target: ClientRenameTarget::NewTab { .. },
            ..
        }))
    ));
    state.handle_raw_events(vec![RawInputEvent::Key(crate::input::TerminalKey::new(
        KeyCode::Esc,
        KeyModifiers::empty(),
    ))]);
    assert!(state.overlay.is_none());
    assert_eq!(state.mode, ClientShellMode::Terminal);

    state.mode = ClientShellMode::Navigate;
    state.compose(44, 20).expect("mobile workspace switcher");
    let new_workspace = state
        .hits
        .mobile_targets
        .iter()
        .find_map(|(rect, target)| {
            matches!(target, ClientMobileTarget::NewWorkspace).then_some(*rect)
        })
        .expect("new workspace hit");
    state.handle_raw_events(vec![click(new_workspace)]);
    assert!(matches!(
        state.overlay,
        Some(ClientShellOverlay::Rename(ClientRenameOverlay {
            target: ClientRenameTarget::NewWorkspace { .. },
            ..
        }))
    ));
    state.handle_raw_events(vec![RawInputEvent::Key(crate::input::TerminalKey::new(
        KeyCode::Esc,
        KeyModifiers::empty(),
    ))]);
    assert!(state.overlay.is_none());
    assert_eq!(state.mode, ClientShellMode::Terminal);

    state.mode = ClientShellMode::Navigate;
    state.compose(44, 20).expect("mobile menu switcher");
    let settings = state
        .hits
        .mobile_targets
        .iter()
        .find_map(|(rect, target)| matches!(target, ClientMobileTarget::Menu(0)).then_some(*rect))
        .expect("settings hit");
    state.handle_raw_events(vec![click(settings)]);
    assert!(matches!(
        state.overlay,
        Some(ClientShellOverlay::Settings(_))
    ));
    state.handle_raw_events(vec![RawInputEvent::Key(crate::input::TerminalKey::new(
        KeyCode::Esc,
        KeyModifiers::empty(),
    ))]);
    assert!(state.overlay.is_none());
    assert_eq!(state.mode, ClientShellMode::Terminal);
}

#[test]
fn mobile_menu_keeps_inert_notes_open_and_cancel_without_workspace_in_navigate() {
    let mut source_config = Config::default();
    source_config.ui.prompt_new_workspace_name = true;
    let config = ClientShellConfig::from_config(&source_config);
    let mut projected = snapshot();
    projected.latest_release_notes_available = true;
    projected.release_notes = None;
    let mut state = ClientShellState::new(config);
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state.mode = ClientShellMode::Navigate;
    state.compose(44, 20).expect("mobile switcher");
    let inert_notes = state
        .hits
        .mobile_targets
        .iter()
        .find_map(|(rect, target)| matches!(target, ClientMobileTarget::Menu(3)).then_some(*rect))
        .expect("what's new row");
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: inert_notes.x,
        row: inert_notes.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert_eq!(state.mode, ClientShellMode::Navigate);
    assert!(state.overlay.is_none());
    assert!(!state.mobile_switcher_suspended);

    let mut empty = snapshot();
    empty.focused_workspace_id = None;
    empty.focused_tab_id = None;
    empty.focused_pane_id = None;
    empty.workspaces.clear();
    empty.tabs.clear();
    empty.panes.clear();
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&source_config));
    state.set_snapshot(Box::new(empty));
    state.set_pane_surface(surface());
    state.mode = ClientShellMode::Navigate;
    state.compose(44, 20).expect("empty mobile switcher");
    let new_workspace = state
        .hits
        .mobile_targets
        .iter()
        .find_map(|(rect, target)| {
            matches!(target, ClientMobileTarget::NewWorkspace).then_some(*rect)
        })
        .expect("new workspace row");
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: new_workspace.x,
        row: new_workspace.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(matches!(state.overlay, Some(ClientShellOverlay::Rename(_))));
    state.handle_raw_events(vec![RawInputEvent::Key(crate::input::TerminalKey::new(
        KeyCode::Esc,
        KeyModifiers::empty(),
    ))]);
    assert_eq!(state.mode, ClientShellMode::Navigate);
}

#[test]
fn mobile_previous_workspace_action_wraps_across_expanded_entries() {
    let mut projected = snapshot();
    for index in 2..=3 {
        projected.workspaces.push(ClientShellWorkspace {
            workspace_id: format!("ws_{index}"),
            active_tab_id: format!("tab_{index}"),
            new_workspace_cwd: "/tmp".into(),
            number: index,
            label: format!("workspace-{index}"),
            custom_label: true,
            branch: None,
            git_ahead_behind: None,
            tokens: Vec::new(),
            worktree: None,
            focused: false,
            agent_status: AgentStatus::Idle,
        });
    }
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state.compose(44, 20).expect("mobile layout");
    let mut outcome = ClientShellInput::default();
    state.record_binding(
        crate::input::KeybindMatch::Action(crate::input::KeybindAction::PreviousWorkspace),
        &mut outcome,
    );
    assert!(outcome.actions.iter().any(|action| matches!(
        action,
        ClientShellAction::Endpoint { request, .. }
            if matches!(
                &request.method,
                crate::api::schema::Method::WorkspaceFocus(target)
                    if target.workspace_id == "ws_3"
            )
    )));
}

#[test]
fn mobile_switcher_scroll_close_and_width_transition_clear_mobile_hits() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    let mut projected = snapshot();
    for index in 2..=8 {
        projected.workspaces.push(ClientShellWorkspace {
            workspace_id: format!("ws_{index}"),
            active_tab_id: format!("tab_{index}"),
            new_workspace_cwd: "/tmp".into(),
            number: index,
            label: format!("workspace-{index}"),
            custom_label: true,
            branch: None,
            git_ahead_behind: None,
            tokens: Vec::new(),
            worktree: None,
            focused: false,
            agent_status: AgentStatus::Idle,
        });
    }
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state.compose(44, 10).expect("mobile header");
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: state.hits.mobile_switch.x,
        row: state.hits.mobile_switch.y,
        modifiers: KeyModifiers::empty(),
    })]);
    state.compose(44, 10).expect("mobile switcher");
    let wheel = state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 20,
        row: 8,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(wheel.repaint);
    assert_eq!(state.mobile_switcher_scroll, 2);
    state.compose(44, 10).expect("wheel position stays stable");
    assert_eq!(state.mobile_switcher_scroll, 2);
    for _ in 0..7 {
        state.handle_raw_events(vec![RawInputEvent::Key(crate::input::TerminalKey::new(
            KeyCode::Down,
            KeyModifiers::empty(),
        ))]);
    }
    state.compose(44, 10).expect("revealed mobile selection");
    assert_eq!(
        state.navigate_workspace_id,
        state.navigation_target(&ClientEndpointId::Local, "ws_8")
    );
    assert!(state.mobile_switcher_scroll > 2);
    assert!(state.hits.mobile_targets.iter().any(|(_, target)| {
        matches!(
            target,
            ClientMobileTarget::Workspace { workspace_id, .. } if workspace_id == "ws_8"
        )
    }));
    let close = state.hits.mobile_close;
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: close.x,
        row: close.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert_eq!(state.mode, ClientShellMode::Terminal);
    assert!(state.navigate_workspace_id.is_none());

    state.compose(80, 20).expect("desktop transition");
    assert!(state.hits.mobile_switch.is_empty());
    assert!(state.hits.mobile_close.is_empty());
    assert!(state.hits.mobile_targets.is_empty());

    state.mode = ClientShellMode::Navigate;
    let short = state.compose(44, 2).expect("short mobile switcher");
    assert_eq!(short.cells[0].symbol, "─");
    assert!(state.hits.mobile_close.is_empty());
    assert!(state.hits.mobile_targets.is_empty());
}

fn mobile_frame_lines(frame: &FrameData) -> Vec<String> {
    frame
        .cells
        .chunks(frame.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

fn mobile_agent(pane_id: &str, workspace_id: &str, status: AgentStatus) -> ClientShellAgent {
    ClientShellAgent {
        pane_id: pane_id.into(),
        workspace_id: workspace_id.into(),
        tab_id: "tab_1".into(),
        name: Some(pane_id.into()),
        display_agent: Some("pi".into()),
        agent: Some("pi".into()),
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: status,
        state_change_seq: 1,
        state_labels: Vec::new(),
        tokens: Vec::new(),
        focused: false,
    }
}

fn mobile_workspace(workspace_id: &str, label: &str) -> ClientShellWorkspace {
    ClientShellWorkspace {
        workspace_id: workspace_id.into(),
        active_tab_id: format!("tab_{workspace_id}"),
        new_workspace_cwd: "/tmp".into(),
        number: 1,
        label: label.into(),
        custom_label: true,
        branch: None,
        git_ahead_behind: None,
        tokens: Vec::new(),
        worktree: None,
        focused: false,
        agent_status: AgentStatus::Idle,
    }
}

fn mobile_agent_targets(state: &ClientShellState) -> Vec<String> {
    state
        .hits
        .mobile_targets
        .iter()
        .filter_map(|(_, target)| match target {
            ClientMobileTarget::Agent { pane_id, .. } => Some(pane_id.clone()),
            _ => None,
        })
        .collect()
}

fn mobile_workspace_targets(state: &ClientShellState) -> Vec<(ClientEndpointId, String)> {
    state
        .hits
        .mobile_targets
        .iter()
        .filter_map(|(_, target)| match target {
            ClientMobileTarget::Workspace {
                endpoint_id,
                workspace_id,
            } => Some((endpoint_id.clone(), workspace_id.clone())),
            _ => None,
        })
        .collect()
}

/// Local (active, with a worktree group), an online `Build` machine and a
/// reconnecting `Jump` machine whose snapshot is cached.
fn three_machine_state() -> (ClientShellState, ClientEndpointId, ClientEndpointId) {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    let profile = |id: &str, label: &str| crate::client::endpoint::SavedSshEndpoint {
        id: crate::client::endpoint::ProfileId::parse(id).unwrap(),
        label: label.into(),
        target: label.to_lowercase(),
        session: "agents".into(),
        enabled: true,
    };
    let build = profile("0123456789abcdef0123456789abcdef", "Build");
    let jump = profile("fedcba9876543210fedcba9876543210", "Jump");
    let build_id = ClientEndpointId::Ssh(build.id.clone());
    let jump_id = ClientEndpointId::Ssh(jump.id.clone());
    state.set_endpoint_catalog(&[build, jump]);
    state.set_endpoint_status(&build_id, ClientEndpointStatus::Online);
    state.set_endpoint_status(&jump_id, ClientEndpointStatus::Online);

    let mut local = snapshot();
    local.workspaces[0].worktree = Some(ClientShellWorktree {
        key: "repo".into(),
        label: "repo".into(),
        is_linked_worktree: false,
    });
    let mut feature = mobile_workspace("ws_2", "repo-feature");
    feature.custom_label = false;
    feature.branch = Some("worktree/feature".into());
    feature.worktree = Some(ClientShellWorktree {
        key: "repo".into(),
        label: "repo".into(),
        is_linked_worktree: true,
    });
    local.workspaces.push(feature);
    let mut fix = mobile_workspace("ws_3", "repo-fix");
    fix.custom_label = false;
    fix.branch = Some("worktree/fix".into());
    fix.worktree = Some(ClientShellWorktree {
        key: "repo".into(),
        label: "repo".into(),
        is_linked_worktree: true,
    });
    local.workspaces.push(fix);
    local.workspaces.push(mobile_workspace("ws_4", "notes"));
    state.set_snapshot(Box::new(local));
    state.set_pane_surface(surface());

    let mut remote = snapshot();
    remote.boot_id = "build-boot".into();
    remote.workspaces[0].label = "api".into();
    remote.workspaces[0].custom_label = true;
    remote.workspaces[0].focused = false;
    remote.workspaces.push(mobile_workspace("ws_2", "web"));
    state.set_endpoint_snapshot(&build_id, Box::new(remote));

    let mut cached = snapshot();
    cached.boot_id = "jump-boot".into();
    cached.workspaces[0].label = "ops".into();
    cached.workspaces[0].custom_label = true;
    cached.workspaces[0].focused = false;
    state.set_endpoint_snapshot(&jump_id, Box::new(cached));
    state.set_endpoint_status(&jump_id, ClientEndpointStatus::Reconnecting);

    for endpoint in &mut state.endpoints {
        let Some(snapshot) = endpoint.snapshot.as_mut() else {
            continue;
        };
        if endpoint.endpoint_id == build_id {
            snapshot.agents = vec![
                mobile_agent("pane_1", "ws_1", AgentStatus::Blocked),
                mobile_agent("pane_2", "ws_2", AgentStatus::Working),
            ];
        } else if endpoint.endpoint_id.is_local() {
            snapshot.agents = vec![
                mobile_agent("pane_1", "ws_1", AgentStatus::Done),
                mobile_agent("pane_9", "ws_4", AgentStatus::Idle),
            ];
        }
    }
    (state, build_id, jump_id)
}

#[test]
fn mobile_switcher_lists_only_agents_that_need_attention() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    let set_agents = |state: &mut ClientShellState, agents: Vec<ClientShellAgent>| {
        state.endpoints[0]
            .snapshot
            .as_mut()
            .expect("local snapshot")
            .agents = agents;
    };
    set_agents(
        &mut state,
        vec![
            mobile_agent("pane_1", "ws_1", AgentStatus::Blocked),
            mobile_agent("pane_2", "ws_1", AgentStatus::Working),
            mobile_agent("pane_3", "ws_1", AgentStatus::Idle),
            mobile_agent("pane_4", "ws_1", AgentStatus::Done),
            mobile_agent("pane_5", "ws_1", AgentStatus::Unknown),
        ],
    );
    state.mode = ClientShellMode::Navigate;
    let frame = state.compose(44, 40).expect("attention switcher");
    let lines = mobile_frame_lines(&frame);
    let text = lines.join("\n");
    let mut agents = mobile_agent_targets(&state);
    agents.sort();
    assert_eq!(agents, ["pane_1", "pane_4"], "{text}");
    let attention = lines
        .iter()
        .position(|line| line.trim() == "attention")
        .expect("attention section");
    let spaces = lines
        .iter()
        .position(|line| line.trim() == "spaces")
        .expect("spaces section");
    assert!(attention < spaces, "{text}");
    // A single machine keeps rows unprefixed and has no machine row.
    assert!(!text.contains("Local"), "{text}");

    set_agents(
        &mut state,
        vec![
            mobile_agent("pane_2", "ws_1", AgentStatus::Working),
            mobile_agent("pane_3", "ws_1", AgentStatus::Idle),
        ],
    );
    let frame = state.compose(44, 40).expect("quiet switcher");
    let text = mobile_frame_lines(&frame).join("\n");
    assert!(mobile_agent_targets(&state).is_empty(), "{text}");
    assert!(!text.contains("attention"), "{text}");

    // An active agent view keeps its label visible even when nothing matches.
    let mut viewed = snapshot();
    viewed.agent_view_label = Some("focus".into());
    state.set_snapshot(Box::new(viewed));
    set_agents(
        &mut state,
        vec![mobile_agent("pane_2", "ws_1", AgentStatus::Working)],
    );
    let frame = state.compose(44, 40).expect("view switcher");
    let text = mobile_frame_lines(&frame).join("\n");
    assert!(text.contains("attention · focus"), "{text}");
    assert!(text.contains("no agents need attention"), "{text}");
}

#[test]
fn mobile_switcher_renders_machine_space_worktree_tree() {
    let (mut state, build_id, jump_id) = three_machine_state();
    state.mode = ClientShellMode::Navigate;
    let frame = state.compose(52, 60).expect("tree switcher");
    let lines = mobile_frame_lines(&frame);
    let text = lines.join("\n");
    if std::env::var_os("HERDR_PRINT_MOBILE_TREE").is_some() {
        eprintln!("{text}");
    }
    let row = |needle: &str| {
        lines
            .iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("missing {needle:?} in\n{text}"))
    };
    let expected = [
        "attention",
        // Blocked leads, then done-not-seen.
        "● Build · api",
        "blocked · pi",
        "● Local · client-shell",
        "done · pi",
        "spaces",
        "+ new workspace",
        "● Local · online",
        "├─ ○ client-shell",
        "│  main · tab 1",
        "│  ├─ ○ feature",
        "│  │    worktree/feature · tab 1",
        "│  └─ ○ fix",
        "│       worktree/fix · tab 1",
        "└─ ○ notes",
        "● Build · online",
        "├─ ○ api",
        "└─ ○ web",
        "◐ Jump · reconnecting",
        "└─ ○ ops",
        "tabs",
        "menu",
    ];
    let mut previous = 0;
    for needle in expected {
        let index = row(needle);
        assert!(index >= previous, "{needle:?} out of order in\n{text}");
        previous = index;
    }
    // Space rows no longer carry a `machine ·` prefix; the working agent on
    // Build and the idle one on Local are not attention items.
    assert!(!text.contains("Build · web"), "{text}");
    assert!(!text.contains("machines"), "{text}");
    let mut agents = mobile_agent_targets(&state);
    agents.sort();
    assert_eq!(agents, ["pane_1", "pane_1"], "{text}");

    // Machine rows stay tappable and keep their Machine target.
    for endpoint_id in [&ClientEndpointId::Local, &build_id, &jump_id] {
        assert!(
            state.hits.mobile_targets.iter().any(
                |(_, target)| matches!(target, ClientMobileTarget::Machine(id) if id == endpoint_id)
            ),
            "missing machine target for {endpoint_id:?}"
        );
    }

    // The reconnecting machine's spaces are dimmed.
    let buffer = frame.to_ratatui_buffer().expect("tree buffer");
    let ops = state
        .hits
        .mobile_targets
        .iter()
        .find_map(|(rect, target)| {
            matches!(
                target,
                ClientMobileTarget::Workspace { endpoint_id, .. } if endpoint_id == &jump_id
            )
            .then_some(*rect)
        })
        .expect("cached jump workspace");
    assert_eq!(
        buffer[(ops.x, ops.y)].fg,
        state.config.palette.overlay0,
        "{text}"
    );
}

#[test]
fn mobile_tree_keyboard_order_matches_rendered_workspace_order() {
    let (mut state, _build_id, jump_id) = three_machine_state();
    state.compose(52, 60).expect("mobile header");
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: state.hits.mobile_switch.x,
        row: state.hits.mobile_switch.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert_eq!(state.mode, ClientShellMode::Navigate);
    state.compose(52, 60).expect("mobile switcher");
    // Keyboard navigation skips machines that are not online.
    let rendered = mobile_workspace_targets(&state)
        .into_iter()
        .filter(|(endpoint_id, _)| endpoint_id != &jump_id)
        .collect::<Vec<_>>();
    assert_eq!(rendered.len(), 6);

    let mut visited = Vec::new();
    for _ in 0..rendered.len() {
        let selected = state
            .navigate_workspace_id
            .clone()
            .expect("selected workspace");
        visited.push((selected.endpoint_id.clone(), selected.workspace_id.clone()));
        state.handle_raw_events(vec![RawInputEvent::Key(crate::input::TerminalKey::new(
            KeyCode::Down,
            KeyModifiers::empty(),
        ))]);
        state.compose(52, 60).expect("moved selection");
    }
    assert_eq!(visited, rendered);
}

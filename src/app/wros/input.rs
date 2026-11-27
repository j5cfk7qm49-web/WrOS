use super::{Page, PowerWait, session::LIVE_INSTALL_CHOICES, shortcuts::SHORTCUT_ACTIONS};
use crate::app::{App, Key, Overlay};
use grimoire_core::tutorial;
use wros_sys::{PowerAction, Request, keyboard::Layout as KeyboardLayout};

impl App {
    pub(crate) fn on_deck_key(&mut self, key: Key) {
        let Overlay::Deck(page) = self.overlay.clone() else {
            return;
        };
        if key == Key::Esc && self.cancel_power_wait() {
            return;
        }
        match page {
            Page::Settings => self.on_settings_key(key),
            Page::LiveInstall { mut sel } => {
                if nav(key, &mut sel, LIVE_INSTALL_CHOICES.len()) {
                    self.overlay = Overlay::Deck(Page::LiveInstall { sel });
                } else if matches!(key, Key::Enter | Key::Esc) {
                    self.overlay = Overlay::None;
                    if key == Key::Enter && sel == 0 {
                        self.request_live_installer();
                    }
                }
            }
            Page::Workspaces { mut sel } => {
                if nav(key, &mut sel, self.deck.state.workspaces.len()) {
                    self.overlay = Overlay::Deck(Page::Workspaces { sel });
                    return;
                }
                match key {
                    Key::Enter => {
                        if !self.commit_saves() || self.project.dirty_count() != 0 {
                            return;
                        }
                        self.remember_deck();
                        self.deck.state.selected = sel;
                        self.restore_deck_position();
                        self.focus_mode = self.deck.state.current().focus && self.open.is_some();
                        if self.focus_mode {
                            self.activate_editor(grimoire_core::documents::Pane::Primary);
                        } else if !self.deck.state.current().outline {
                            self.focus = self.editor_focus();
                        }
                        self.deck.dirty = true;
                        self.save_deck();
                        self.overlay = Overlay::None;
                        self.observe_deck(tutorial::Event::WorkspaceSelected);
                    }
                    Key::Char('+' | '-') => {
                        let w = &mut self.deck.state.workspaces[sel];
                        w.outline_width = if key == Key::Char('+') {
                            (w.outline_width + 2).min(60)
                        } else {
                            w.outline_width.saturating_sub(2).max(18)
                        };
                        self.deck.dirty = true;
                        self.deck.save_layout();
                    }
                    Key::Char('<' | '>') => {
                        let w = &mut self.deck.state.workspaces[sel];
                        w.secondary_percent = if key == Key::Char('>') {
                            (w.secondary_percent + 5).min(75)
                        } else {
                            w.secondary_percent.saturating_sub(5).max(25)
                        };
                        self.deck.dirty = true;
                        self.deck.save_layout();
                    }
                    Key::Char('b') => self.starter_preset_picker(sel),
                    Key::Char('p') => {
                        let ids = self.deck.state.workspaces[sel]
                            .pin_ids
                            .clone()
                            .unwrap_or_else(|| {
                                self.pins.state().pins.iter().map(|p| p.id).collect()
                            });
                        self.overlay = Overlay::Deck(Page::WorkspacePins {
                            workspace: sel,
                            sel: 0,
                            ids,
                        });
                    }
                    Key::Char('n') => {
                        if self.deck.state.workspaces.len() < 64 {
                            let mut w = self.deck.state.current().clone();
                            let mut n = 1;
                            while self
                                .deck
                                .state
                                .workspaces
                                .iter()
                                .any(|w| w.name == format!("Custom {n}"))
                            {
                                n += 1;
                            }
                            w.name = format!("Custom {n}");
                            self.deck.state.workspaces.push(w);
                            self.deck.dirty = true;
                            self.deck.save_layout();
                        }
                    }
                    Key::Esc => self.overlay = Overlay::None,
                    _ => {}
                }
            }
            Page::WorkspacePins {
                workspace,
                mut sel,
                mut ids,
            } => {
                if nav(key, &mut sel, self.pins.state().pins.len()) {
                    self.overlay = Overlay::Deck(Page::WorkspacePins {
                        workspace,
                        sel,
                        ids,
                    });
                    return;
                }
                match key {
                    Key::Char(' ') => {
                        if let Some(pin) = self.pins.state().pins.get(sel) {
                            if ids.contains(&pin.id) {
                                ids.retain(|id| *id != pin.id);
                            } else {
                                ids.push(pin.id);
                            }
                        }
                        self.overlay = Overlay::Deck(Page::WorkspacePins {
                            workspace,
                            sel,
                            ids,
                        });
                    }
                    Key::Char('z') => {
                        self.overlay = Overlay::Deck(Page::WorkspacePins {
                            workspace,
                            sel,
                            ids: vec![],
                        })
                    }
                    Key::Char('a') => {
                        self.overlay = Overlay::Deck(Page::WorkspacePins {
                            workspace,
                            sel,
                            ids: self.pins.state().pins.iter().map(|p| p.id).collect(),
                        })
                    }
                    Key::Enter | Key::Char('A') => {
                        let result = self.save_workspace_pin_set(
                            workspace,
                            if key == Key::Char('A') {
                                None
                            } else {
                                Some(ids)
                            },
                        );
                        self.deck.note = match result {
                            Ok(()) => {
                                "Workspace pin set saved; global cards/sources unchanged".into()
                            }
                            Err(e) => format!("Pin set not saved: {e:#}"),
                        };
                        self.overlay = Overlay::Deck(Page::Workspaces { sel: workspace });
                    }
                    Key::Esc => self.overlay = Overlay::Deck(Page::Workspaces { sel: workspace }),
                    _ => {}
                }
            }
            Page::System { mut sel } => {
                if nav(key, &mut sel, 12) {
                    self.overlay = Overlay::Deck(Page::System { sel });
                    return;
                }
                if key == Key::Esc {
                    self.overlay = Overlay::None;
                    return;
                }
                if key != Key::Enter {
                    return;
                }
                match sel {
                    0 => self.deck.request(Request::Refresh),
                    1 => self.deck.request(Request::Wifi(true)),
                    2 => self.deck.request(Request::Wifi(false)),
                    3 => self.deck.request(Request::ScanWifi),
                    4 => {
                        self.deck.system_parent = Some(sel);
                        self.overlay = Overlay::Deck(Page::Keyboard { sel: 0 });
                    }
                    5 | 6 => {
                        let now = self
                            .deck
                            .snapshot
                            .as_ref()
                            .and_then(|s| s.brightness_percent);
                        if let Some(now) = now {
                            self.deck.request(Request::Brightness(if sel == 5 {
                                now.saturating_sub(10).max(1)
                            } else {
                                now.saturating_add(10).min(100)
                            }));
                        } else {
                            self.deck.note =
                                "No supported backlight is available; brightness unchanged.".into();
                        }
                    }
                    7 => {
                        self.deck.state.low_power = !self.deck.state.low_power;
                        self.deck.dirty = true;
                        self.save_deck();
                        self.deck
                            .request(Request::PowerSave(self.deck.state.low_power));
                    }
                    8 => {
                        if self.deck.suspend_available {
                            self.overlay = Overlay::Deck(Page::Power {
                                action: PowerAction::Suspend,
                                yes: false,
                            });
                        } else {
                            self.deck.note = wros_sys::SUSPEND_UNAVAILABLE.into();
                            self.deck.shell.notices.push(&self.deck.note);
                        }
                    }
                    9..=10 => {
                        self.overlay = Overlay::Deck(Page::Power {
                            action: match sel {
                                9 => PowerAction::Shutdown,
                                _ => PowerAction::Restart,
                            },
                            yes: false,
                        })
                    }
                    11 => {
                        self.deck.system_parent = Some(sel);
                        self.overlay = Overlay::Deck(Page::Shortcuts {
                            sel: 0,
                            capture: false,
                        })
                    }
                    _ => {}
                }
            }
            Page::Keyboard { mut sel } => {
                if nav(key, &mut sel, KeyboardLayout::ALL.len()) {
                    self.overlay = Overlay::Deck(Page::Keyboard { sel });
                    return;
                }
                match key {
                    Key::Enter if !self.deck.busy => {
                        self.deck
                            .request(Request::KeyboardPreview(KeyboardLayout::ALL[sel].clone()));
                        if self.deck.busy {
                            self.overlay = Overlay::Deck(Page::KeyboardWait);
                        }
                    }
                    Key::Esc => self.close_system_subpage(),
                    _ => {}
                }
            }
            Page::KeyboardWait => {
                if key == Key::Esc {
                    self.overlay = if self.deck.shell.external {
                        Overlay::None
                    } else {
                        Overlay::Deck(Page::Keyboard { sel: 0 })
                    };
                }
            }
            Page::KeyboardTest { mut text } => match key {
                Key::Enter | Key::Esc => {
                    if let Some((token, _, _)) = self.deck.preview.take() {
                        self.deck.request(if key == Key::Enter {
                            Request::KeyboardConfirm(token)
                        } else {
                            Request::KeyboardCancel(token)
                        });
                    }
                    self.overlay = if self.deck.shell.external {
                        Overlay::None
                    } else {
                        Overlay::Deck(Page::Keyboard { sel: 0 })
                    };
                }
                Key::Char(c) if !c.is_control() && text.chars().count() < 160 => {
                    text.push(c);
                    self.overlay = Overlay::Deck(Page::KeyboardTest { text });
                }
                Key::Backspace => {
                    text.pop();
                    self.overlay = Overlay::Deck(Page::KeyboardTest { text });
                }
                _ => {}
            },
            Page::Power { action, mut yes } => match key {
                Key::Left | Key::Right | Key::Tab => {
                    yes = !yes;
                    self.overlay = Overlay::Deck(Page::Power { action, yes });
                }
                Key::Esc => self.open_system_settings(),
                Key::Enter => {
                    if yes && action == PowerAction::Suspend {
                        if let Some(what) = self.sleep_blocker() {
                            // Wait for it, said on screen; Esc cancels (#168).
                            self.power_hold(PowerWait::Sleep, what);
                            self.open_system_settings();
                            return;
                        }
                        if !self.request_suspend() {
                            return;
                        }
                    } else if yes {
                        let kind = if action == PowerAction::Restart {
                            PowerWait::Restart
                        } else {
                            PowerWait::Shutdown
                        };
                        if !self.power_off(kind) {
                            // Refused: the confirmation stays for an explicit retry.
                            return;
                        }
                    }
                    self.open_system_settings();
                }
                _ => {}
            },
            Page::Tutorial { mut sel } => {
                if nav(key, &mut sel, tutorial::lessons().len()) {
                    self.overlay = Overlay::Deck(Page::Tutorial { sel });
                    return;
                }
                let lesson = tutorial::lessons().get(sel);
                match key {
                    Key::Enter => {
                        if let Some(lesson) = lesson {
                            self.overlay = Overlay::Deck(Page::Lesson { id: lesson.id });
                        }
                    }
                    Key::Char('s') => {
                        if let Some(lesson) = lesson {
                            self.deck.progress.skip(lesson.id);
                            self.deck.save_progress();
                        }
                    }
                    Key::Char('d') => self.start_tutorial_demo(),
                    Key::Esc => self.overlay = Overlay::None,
                    _ => {}
                }
            }
            Page::Lesson { id } => match key {
                Key::Enter => self.start_deck_lesson(id),
                Key::Esc => {
                    self.overlay = Overlay::Deck(Page::Tutorial {
                        sel: usize::from(id) - 1,
                    })
                }
                Key::Char('h') => {
                    let topic = &tutorial::lessons()[usize::from(id) - 1].help_topic;
                    let topic = crate::help::TOPICS
                        .iter()
                        .find(|t| t.id == topic)
                        .map_or("writingdeck", |t| t.id);
                    self.open_help(Some(topic));
                }
                _ => {}
            },
            Page::Shortcuts { mut sel, capture } => {
                if capture {
                    if key == Key::Esc {
                        self.overlay = Overlay::Deck(Page::Shortcuts {
                            sel,
                            capture: false,
                        });
                    }
                    return;
                }
                if nav(key, &mut sel, SHORTCUT_ACTIONS.len()) {
                    self.overlay = Overlay::Deck(Page::Shortcuts { sel, capture });
                    return;
                }
                match key {
                    Key::Enter => {
                        self.overlay = Overlay::Deck(Page::Shortcuts { sel, capture: true })
                    }
                    Key::Esc => self.close_system_subpage(),
                    _ => {}
                }
            }
        }
    }
}

fn nav(key: Key, sel: &mut usize, len: usize) -> bool {
    if len == 0 {
        return false;
    }
    let next = match key {
        Key::Up => sel.saturating_sub(1),
        Key::Down => (*sel + 1).min(len - 1),
        Key::Home => 0,
        Key::End => len - 1,
        Key::PageUp => sel.saturating_sub(10),
        Key::PageDown => (*sel + 10).min(len - 1),
        _ => return false,
    };
    *sel = next;
    true
}

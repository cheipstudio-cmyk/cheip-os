//! Telefono e Messaggi: rubrica, registro, chiamate e SMS via ModemManager.

use crate::controller::{initial, model, push_apps, push_notifs, when, CallState, Shared, State};
use crate::modem;
use crate::store::Message;
use crate::with;
use crate::*;
use chrono::Local;
use slint::ComponentHandle;

fn now_ts() -> i64 {
    Local::now().timestamp()
}

fn name_or_number(st: &State, number: &str) -> String {
    st.store.contact_name(number).unwrap_or_else(|| number.to_string())
}

pub fn push_phone(ui: &AppWindow, st: &State) {
    let now = Local::now().naive_local();
    let p = ui.global::<Phone>();
    p.set_modem_ok(st.modem_ok);
    p.set_contacts(model(
        st.store
            .data
            .contacts
            .iter()
            .map(|c| ContactRow { name: c.name.clone().into(), number: c.number.clone().into(), initial: initial(&c.name) })
            .collect(),
    ));
    p.set_recents(model(
        st.store
            .data
            .calls
            .iter()
            .map(|c| CallRow {
                name: name_or_number(st, &c.number).into(),
                number: c.number.clone().into(),
                when: when(c.ts, now).into(),
                dir: c.dir.clone().into(),
            })
            .collect(),
    ));
}

pub fn push_threads(ui: &AppWindow, st: &State) {
    let now = Local::now().naive_local();
    let m = ui.global::<Msg>();
    m.set_modem_ok(st.modem_ok);
    m.set_threads(model(
        st.store
            .threads()
            .into_iter()
            .map(|(number, last, unread)| {
                let name = name_or_number(st, &number);
                ThreadRow {
                    initial: initial(&name),
                    name: name.into(),
                    number: number.into(),
                    last: (if last.outgoing { format!("You: {}", last.text) } else { last.text }).into(),
                    when: when(last.ts, now).into(),
                    unread: unread as i32,
                }
            })
            .collect(),
    ));
}

pub fn push_conversation(ui: &AppWindow, st: &State) {
    let now = Local::now().naive_local();
    let m = ui.global::<Msg>();
    m.set_open_number(st.open_thread.clone().into());
    m.set_open_name(name_or_number(st, &st.open_thread).into());
    let mut msgs: Vec<&Message> =
        st.store.data.messages.iter().filter(|x| crate::store::same_number(&x.number, &st.open_thread)).collect();
    msgs.sort_by_key(|x| x.ts);
    m.set_messages(model(
        msgs.into_iter().map(|x| Bubble { text: x.text.clone().into(), when: when(x.ts, now).into(), out: x.outgoing }).collect(),
    ));
}

pub fn push(ui: &AppWindow, st: &State) {
    push_phone(ui, st);
    push_threads(ui, st);
    push_conversation(ui, st);
    ui.global::<Msg>().set_status(
        (if st.modem_ok || !st.live { "" } else { "The modem on this phone is not active yet. Texts will arrive once it is." }).into(),
    );
}

fn show_call(ui: &AppWindow, st: &mut State) {
    let ringing_now = st.call.as_ref().map(|c| c.incoming && c.answered_at.is_none()).unwrap_or(false);
    if ringing_now {
        crate::controller::start_ring(st);
    } else {
        crate::controller::stop_ring(st);
    }
    let p = ui.global::<Phone>();
    match &st.call {
        Some(c) => {
            let name = name_or_number(st, &c.number);
            p.set_call_initial(initial(&name));
            p.set_call_name(name.into());
            p.set_call_number(c.number.clone().into());
            let ringing = c.incoming && c.answered_at.is_none();
            p.set_incoming(ringing);
            p.set_in_call(!ringing);
            match c.answered_at {
                Some(t) => {
                    let secs = (now_ts() - t).max(0);
                    p.set_call_time(format!("{:02}:{:02}", secs / 60, secs % 60).into());
                }
                None => p.set_call_time("".into()),
            }
            if !st.modem_ok && st.live {
                p.set_call_state("Modem not available".into());
            } else if c.answered_at.is_none() && !c.incoming {
                p.set_call_state("Calling…".into());
            }
        }
        None => {
            p.set_incoming(false);
            p.set_in_call(false);
        }
    }
}

/// Fine chiamata: registro, eventuale notifica di chiamata persa
fn end_call(ui: &AppWindow, st: &mut State) {
    let Some(c) = st.call.take() else { return };
    if !c.number.is_empty() && (st.modem_ok || !st.live) {
        let (dir, secs) = match (c.incoming, c.answered_at) {
            (true, None) => ("missed", 0),
            (true, Some(t)) => ("in", now_ts() - t),
            (false, Some(t)) => ("out", now_ts() - t),
            (false, None) => ("out", 0),
        };
        st.store.log_call(&c.number, c.started_at, dir, secs);
        st.store.save();
        if dir == "missed" {
            st.missed += 1;
            let who = name_or_number(st, &c.number);
            crate::controller::post(ui, st, "phone", "Missed call", &who, crate::feedback::Sound::Notification);
        }
    }
    show_call(ui, st);
    push_phone(ui, st);
}

fn store_incoming(ui: &AppWindow, st: &mut State, list: Vec<modem::Sms>) {
    let mut fresh = 0;
    for sms in list {
        let ts = chrono::DateTime::parse_from_rfc3339(&sms.timestamp).map(|d| d.timestamp()).unwrap_or_else(|_| now_ts());
        let is_open = crate::store::same_number(&sms.number, &st.open_thread);
        let added = st.store.add_message(Message { number: sms.number.clone(), text: sms.text.clone(), ts, outgoing: false, read: is_open });
        if added {
            fresh += 1;
            let who = name_or_number(st, &sms.number);
            crate::controller::post(ui, st, "messages", &who, &sms.text, crate::feedback::Sound::Message);
        }
        // Salvato in locale: lo togliamo dalla SIM/modem che ha poco spazio
        modem::delete_sms(&sms.path);
    }
    if fresh > 0 {
        st.store.save();
        push_threads(ui, st);
        push_conversation(ui, st);
        push_notifs(ui, st);
        push_apps(ui, st);
    }
}

/// Demo: chiamata in arrivo finta
pub fn simulate_incoming(ui: &AppWindow, st: &mut State, number: &str) {
    if st.call.is_some() {
        return;
    }
    st.call = Some(CallState { number: number.into(), incoming: true, started_at: now_ts(), ..Default::default() });
    show_call(ui, st);
}

/// Demo: SMS in arrivo finto
pub fn simulate_message(ui: &AppWindow, st: &mut State, number: &str, text: &str) {
    let is_open = crate::store::same_number(number, &st.open_thread);
    st.store.add_message(Message { number: number.into(), text: text.into(), ts: now_ts(), outgoing: false, read: is_open });
    let who = name_or_number(st, number);
    push_threads(ui, st);
    push_conversation(ui, st);
    crate::controller::post(ui, st, "messages", &who, text, crate::feedback::Sound::Message);
}

pub fn wire(ui: &AppWindow, st: &Shared) {
    let p = ui.global::<Phone>();
    p.on_add_contact(with!(ui, st, |ui, s, name, number| {
        s.store.add_contact(name.trim(), number.trim());
        s.store.save();
        push_phone(&ui, &s);
        push_threads(&ui, &s);
    }));
    p.on_call(with!(ui, st, |ui, s, number| {
        if s.call.is_some() || number.trim().is_empty() {
            return;
        }
        s.call = Some(CallState { number: number.to_string(), started_at: now_ts(), ..Default::default() });
        if s.live && s.modem_ok {
            let path = modem::dial(&number).unwrap_or_default();
            if let Some(c) = s.call.as_mut() {
                c.path = path;
            }
        } else if !s.live {
            // Screenshot e test: chiamata simulata subito attiva
            if let Some(c) = s.call.as_mut() {
                c.answered_at = Some(now_ts() - 83);
            }
        }
        ui.global::<Phone>().set_dial("".into());
        show_call(&ui, &mut s);
    }));
    p.on_answer(with!(ui, st, |ui, s| {
        let live = s.live;
        if let Some(c) = s.call.as_mut() {
            if live {
                modem::accept(&c.path);
            }
            c.answered_at = Some(now_ts());
        }
        show_call(&ui, &mut s);
    }));
    p.on_hangup(with!(ui, st, |ui, s| {
        if let Some(c) = &s.call {
            if s.live && !c.path.is_empty() {
                modem::hangup(&c.path);
            }
        }
        end_call(&ui, &mut s);
    }));

    let m = ui.global::<Msg>();
    m.on_open_thread(with!(ui, st, |ui, s, number| {
        s.open_thread = number.to_string();
        s.store.conversation(&number);
        s.store.save();
        push_threads(&ui, &s);
        push_conversation(&ui, &s);
        push_apps(&ui, &s);
    }));
    m.on_close_thread(with!(ui, st, |ui, s| {
        s.open_thread.clear();
        push_conversation(&ui, &s);
    }));
    m.on_send(with!(ui, st, |ui, s, number, text| {
        let (number, text) = (number.trim().to_string(), text.trim().to_string());
        if number.is_empty() || text.is_empty() {
            return;
        }
        if s.live && !s.modem_ok {
            ui.global::<Msg>().set_status("Modem not available, message not sent".into());
            return;
        }
        if s.live {
            let weak = ui.as_weak();
            let (n2, t2) = (number.clone(), text.clone());
            std::thread::spawn(move || {
                let ok = modem::send_sms(&n2, &t2);
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(ui) = weak.upgrade() else { return };
                    let Some(st2) = crate::controller::shared() else { return };
                    let mut s = st2.borrow_mut();
                    if ok {
                        s.store.add_message(Message { number: n2, text: t2, ts: now_ts(), outgoing: true, read: true });
                        s.store.save();
                        push_threads(&ui, &s);
                        push_conversation(&ui, &s);
                    } else {
                        ui.global::<Msg>().set_status("Sending failed".into());
                    }
                });
            });
        } else {
            s.store.add_message(Message { number: number.clone(), text, ts: now_ts(), outgoing: true, read: true });
        }
        s.open_thread = number;
        push_threads(&ui, &s);
        push_conversation(&ui, &s);
    }));
}

/// Polling del modem: stato ogni 5 s, chiamate ogni secondo, SMS ogni 15 s
pub fn timers(ui: &AppWindow, st: &Shared) -> Vec<slint::Timer> {
    if !st.borrow().live {
        return vec![];
    }
    let status = slint::Timer::default();
    status.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(5),
        with!(ui, st, |ui, s| {
            // Read on the background thread (mmcli is slow), never here
            let Some(ms) = crate::controller::hw_snapshot().and_then(|h| h.modem) else { return };
            let ok = ms.present;
            let p = ui.global::<Phone>();
            p.set_modem_label((if ok { ms.operator.clone() } else { "Modem not available".to_string() }).into());
            let set = ui.global::<Settings>();
            set.set_modem_operator(ms.operator.into());
            set.set_modem_signal(if ok { format!("{}%", ms.signal) } else { "--".into() }.into());
            set.set_modem_state(ms.state.into());
            if ok != s.modem_ok {
                s.modem_ok = ok;
                push(&ui, &s);
            }
        }),
    );
    let calls = slint::Timer::default();
    calls.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(1),
        with!(ui, st, |ui, s| {
            if !s.modem_ok {
                return;
            }
            let list = modem::calls();
            let active = list.into_iter().find(|c| c.state != "terminated" && c.state != "unknown");
            match (s.call.is_some(), active) {
                (false, Some(c)) if c.incoming && c.state == "ringing-in" => {
                    s.call = Some(CallState { path: c.path, number: c.number, incoming: true, answered_at: None, started_at: now_ts() });
                }
                (true, Some(c)) => {
                    if let Some(cs) = s.call.as_mut() {
                        if cs.path.is_empty() {
                            cs.path = c.path.clone();
                        }
                        if c.state == "active" && cs.answered_at.is_none() {
                            cs.answered_at = Some(now_ts());
                        }
                    }
                }
                (true, None) => {
                    end_call(&ui, &mut s);
                    return;
                }
                _ => return,
            }
            show_call(&ui, &mut s);
        }),
    );
    let sms = slint::Timer::default();
    sms.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(15),
        with!(ui, st, |ui, s| {
            if s.modem_ok {
                let list = modem::received_sms();
                store_incoming(&ui, &mut s, list);
            }
        }),
    );
    vec![status, calls, sms]
}

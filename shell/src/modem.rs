//! Chiamate e SMS tramite ModemManager (mmcli). Tutto passa da qui, così
//! quando il modem dei Lumia funzionerà in mainline la shell è già pronta.

use std::collections::HashMap;
use std::process::Command;

fn mmcli(args: &[&str]) -> Option<String> {
    let out = Command::new("mmcli").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Output "chiave : valore" di mmcli -K
pub fn parse_kv(s: &str) -> HashMap<String, String> {
    s.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(" : ")?;
            Some((k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}

/// Percorsi D-Bus in un elenco di mmcli (Call, SMS, Modem)
pub fn parse_paths(s: &str, kind: &str) -> Vec<String> {
    let needle = format!("/org/freedesktop/ModemManager1/{kind}/");
    s.split_whitespace().filter(|w| w.starts_with(&needle)).map(|w| w.to_string()).collect()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub present: bool,
    pub operator: String,
    pub signal: i32,
    pub state: String,
    pub tech: String,
}

pub fn status() -> Status {
    let Some(list) = mmcli(&["-L"]) else { return Status::default() };
    if parse_paths(&list, "Modem").is_empty() {
        return Status::default();
    }
    let kv = mmcli(&["-m", "any", "-K"]).map(|s| parse_kv(&s)).unwrap_or_default();
    status_from(&kv)
}

pub fn status_from(kv: &HashMap<String, String>) -> Status {
    let get = |k: &str| kv.get(k).cloned().filter(|v| v != "--").unwrap_or_default();
    Status {
        present: true,
        operator: get("modem.3gpp.operator-name"),
        signal: get("modem.generic.signal-quality.value").parse().unwrap_or(0),
        state: get("modem.generic.state"),
        tech: get("modem.generic.access-technologies.value[1]"),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub path: String,
    pub number: String,
    pub incoming: bool,
    pub state: String,
}

pub fn call_from(path: &str, kv: &HashMap<String, String>) -> Call {
    let get = |k: &str| kv.get(k).cloned().unwrap_or_default();
    Call {
        path: path.to_string(),
        number: get("call.properties.number"),
        incoming: get("call.properties.direction") == "incoming",
        state: get("call.properties.state"),
    }
}

pub fn calls() -> Vec<Call> {
    let Some(list) = mmcli(&["-m", "any", "--voice-list-calls"]) else { return vec![] };
    parse_paths(&list, "Call")
        .into_iter()
        .filter_map(|p| mmcli(&["-o", &p, "-K"]).map(|s| call_from(&p, &parse_kv(&s))))
        .collect()
}

/// Avvia una chiamata, restituisce il percorso D-Bus
pub fn dial(number: &str) -> Option<String> {
    let clean: String = number.chars().filter(|c| c.is_ascii_digit() || *c == '+' || *c == '*' || *c == '#').collect();
    if clean.is_empty() {
        return None;
    }
    let out = mmcli(&["-m", "any", &format!("--voice-create-call=number={clean}")])?;
    let path = parse_paths(&out, "Call").into_iter().next()?;
    mmcli(&["-o", &path, "--start"])?;
    Some(path)
}

pub fn accept(path: &str) -> bool {
    mmcli(&["-o", path, "--accept"]).is_some()
}

pub fn hangup(path: &str) -> bool {
    mmcli(&["-o", path, "--hangup"]).is_some()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sms {
    pub path: String,
    pub number: String,
    pub text: String,
    /// RFC 3339 dal modem
    pub timestamp: String,
    pub outgoing: bool,
}

pub fn sms_from(path: &str, kv: &HashMap<String, String>) -> Sms {
    let get = |k: &str| kv.get(k).cloned().unwrap_or_default();
    Sms {
        path: path.to_string(),
        number: get("sms.content.number"),
        text: get("sms.content.text"),
        timestamp: get("sms.properties.timestamp"),
        outgoing: get("sms.properties.pdu-type") == "submit",
    }
}

/// SMS ricevuti completi presenti nel modem
pub fn received_sms() -> Vec<Sms> {
    let Some(list) = mmcli(&["-m", "any", "--messaging-list-sms"]) else { return vec![] };
    parse_paths(&list, "SMS")
        .into_iter()
        .filter_map(|p| {
            let kv = parse_kv(&mmcli(&["-s", &p, "-K"])?);
            (kv.get("sms.properties.state").map(String::as_str) == Some("received")).then(|| sms_from(&p, &kv))
        })
        .collect()
}

pub fn delete_sms(path: &str) {
    mmcli(&["-m", "any", &format!("--messaging-delete-sms={path}")]);
}

/// mmcli separa i campi con la virgola e usa gli apici per i valori:
/// gli apici nel testo diventano apostrofi tipografici
pub fn sms_create_arg(number: &str, text: &str) -> String {
    let text = text.replace('\'', "\u{2019}");
    let number: String = number.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect();
    format!("--messaging-create-sms=text='{text}',number='{number}'")
}

pub fn send_sms(number: &str, text: &str) -> bool {
    let Some(out) = mmcli(&["-m", "any", &sms_create_arg(number, text)]) else { return false };
    let Some(path) = parse_paths(&out, "SMS").into_iter().next() else { return false };
    mmcli(&["-s", &path, "--send"]).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEM: &str = "modem.generic.state                          : registered
modem.generic.signal-quality.value           : 67
modem.generic.access-technologies.value[1]   : lte
modem.3gpp.operator-name                     : TIM";

    #[test]
    fn modem_status() {
        let s = status_from(&parse_kv(MODEM));
        assert_eq!(s.operator, "TIM");
        assert_eq!(s.signal, 67);
        assert_eq!(s.state, "registered");
        assert_eq!(s.tech, "lte");
    }

    #[test]
    fn call_and_paths() {
        let list = "    /org/freedesktop/ModemManager1/Call/3 incoming (ringing-in)\n";
        assert_eq!(parse_paths(list, "Call"), vec!["/org/freedesktop/ModemManager1/Call/3"]);
        let kv = parse_kv("call.properties.number    : +393331234567\ncall.properties.direction : incoming\ncall.properties.state     : ringing-in");
        let c = call_from("/x", &kv);
        assert!(c.incoming);
        assert_eq!(c.state, "ringing-in");
        assert_eq!(c.number, "+393331234567");
    }

    #[test]
    fn sms_parse_and_escape() {
        let kv = parse_kv("sms.content.number : +39333\nsms.content.text : Ci vediamo alle 8 : ok\nsms.properties.pdu-type : deliver\nsms.properties.timestamp : 2026-11-25T22:47:00+01:00");
        let s = sms_from("/s", &kv);
        assert_eq!(s.text, "Ci vediamo alle 8 : ok");
        assert!(!s.outgoing);
        assert_eq!(sms_create_arg("+39 333-1", "l'ora"), "--messaging-create-sms=text='l\u{2019}ora',number='+393331'");
    }
}

//! Leitura do estado do ufw e classificação de exposição de cada porta.
use crate::collect::{Proto, Scope};
use std::fs;

pub struct Firewall {
    pub enabled: bool,
    pub drop_by_default: bool,
    /// None quando as regras não podem ser lidas (precisa de root).
    pub rules: Option<Vec<(String, u16, u16)>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Exposure {
    Local,
    Blocked,
    BlockedAssumed,
    Allowed,
    NoFirewall,
    Docker,
}

impl Exposure {
    pub fn is_exposed(self) -> bool {
        matches!(self, Exposure::Allowed | Exposure::NoFirewall | Exposure::Docker)
    }
}

pub fn load() -> Firewall {
    let enabled = fs::read_to_string("/etc/ufw/ufw.conf")
        .map(|s| s.lines().any(|l| l.trim().eq_ignore_ascii_case("ENABLED=yes")))
        .unwrap_or(false);
    let drop_by_default = fs::read_to_string("/etc/default/ufw")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("DEFAULT_INPUT_POLICY"))
                .map(|l| !l.contains("ACCEPT"))
        })
        .unwrap_or(true);
    let mut rules = Vec::new();
    let mut readable = true;
    for f in ["/etc/ufw/user.rules", "/etc/ufw/user6.rules"] {
        match fs::read_to_string(f) {
            Ok(s) => {
                for l in s.lines().filter(|l| l.starts_with("### tuple ### allow")) {
                    let t: Vec<&str> = l.split_whitespace().collect();
                    let (Some(proto), Some(ports)) = (t.get(4), t.get(5)) else { continue };
                    if *ports == "any" {
                        rules.push((proto.to_string(), 0, u16::MAX));
                        continue;
                    }
                    for part in ports.split(',') {
                        let mut r = part.split(':');
                        let lo: u16 = r.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                        let hi: u16 = r.next().and_then(|x| x.parse().ok()).unwrap_or(lo);
                        rules.push((proto.to_string(), lo, hi));
                    }
                }
            }
            Err(_) => readable = false,
        }
    }
    Firewall { enabled, drop_by_default, rules: readable.then_some(rules) }
}

impl Firewall {
    fn allows(&self, port: u16, proto: Proto) -> bool {
        self.rules.as_ref().is_some_and(|rs| {
            rs.iter().any(|(p, lo, hi)| (p == proto.as_str() || p == "any") && port >= *lo && port <= *hi)
        })
    }

    pub fn exposure(&self, scope: Scope, port: u16, proto: Proto, docker: bool) -> Exposure {
        if scope == Scope::Local {
            Exposure::Local
        } else if docker {
            Exposure::Docker
        } else if !self.enabled || !self.drop_by_default {
            Exposure::NoFirewall
        } else if self.allows(port, proto) {
            Exposure::Allowed
        } else if self.rules.is_some() {
            Exposure::Blocked
        } else {
            Exposure::BlockedAssumed
        }
    }
}

//! FTN domain handling, the port of `ftndom.c`.

use crate::addr::FtnAddr;

/// An entry of the FTN domain table.
#[derive(Clone, Debug)]
pub struct FtnDomain {
    /// The domain name, for example `fidonet`.
    pub name: String,
    /// The path of the outbound directory.
    pub path: String,
    /// The name of the outbound directory.
    pub dir: String,
    /// The default zones of the domain.
    pub z: [i32; 2],
    /// The internet root domain, for example `other.net`.
    pub idomain: Option<String>,
    /// The index of the 4D alias of the domain.
    pub alias4: Option<usize>,
}

/// The domain table.
#[derive(Clone, Debug, Default)]
pub struct DomainTable {
    domains: Vec<FtnDomain>,
}

impl DomainTable {
    /// Creates a table from the domains.
    pub fn new(domains: Vec<FtnDomain>) -> DomainTable {
        DomainTable { domains }
    }

    /// Returns the domains.
    pub fn domains(&self) -> &[FtnDomain] {
        &self.domains
    }

    /// Returns the domain by name. The lookup ignores case.
    pub fn get(&self, name: &str) -> Option<&FtnDomain> {
        self.domains
            .iter()
            .find(|d| d.name.eq_ignore_ascii_case(name))
    }

    /// Returns the domain that `alias4` points to.
    pub fn resolved<'a>(&'a self, d: &'a FtnDomain) -> &'a FtnDomain {
        match d.alias4 {
            Some(i) if i < self.domains.len() => &self.domains[i],
            _ => d,
        }
    }

    /// Returns the matched domain by zone.
    ///
    /// The C code logs a message when it guesses the domain.
    pub fn matched(&self, zone: i32, aka: &[FtnAddr]) -> String {
        // Is it the default zone of a domain?
        for d in &self.domains {
            if d.alias4.is_none() && d.z[0] == zone {
                return d.name.clone();
            }
        }

        // Do we have an AKA with this zone?
        for a in aka {
            if a.z == zone {
                return a.domain.clone();
            }
        }

        // No defined domain: guess the defaults.
        if let Some(first) = aka.first() {
            first.domain.clone()
        } else {
            self.domains
                .first()
                .map(|d| d.name.clone())
                .unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn domains() -> DomainTable {
        DomainTable::new(vec![
            FtnDomain {
                name: "fidonet".to_string(),
                path: "/tmp/fido".to_string(),
                dir: "outb".to_string(),
                z: [2, -1],
                idomain: None,
                alias4: None,
            },
            FtnDomain {
                name: "othernet".to_string(),
                path: "/tmp/other".to_string(),
                dir: "outb".to_string(),
                z: [3, -1],
                idomain: Some("other.net".to_string()),
                alias4: None,
            },
        ])
    }

    #[test]
    fn lookup() {
        let d = domains();
        assert_eq!(d.get("FIDONET").unwrap().name, "fidonet");
        assert!(d.get("nosuchnet").is_none());
    }

    #[test]
    fn matched() {
        let d = domains();
        assert_eq!(d.matched(2, &[]), "fidonet");
        assert_eq!(d.matched(3, &[]), "othernet");
    }
}

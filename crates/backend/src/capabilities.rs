//! Capability catalog — the source of truth for what pointe.dev can actually
//! DELIVER, and at what tier. It gates the designer (and is enforced by the design
//! critic) so a blueprint never promises an integration we can't build. Honesty by
//! construction: the designer may only present an integration as turnkey if it is
//! cataloged Native or HTTP; anything bespoke/internal is forced into Managed.
//!
//! Two orthogonal axes per service:
//! - `Tier`  — can we BUILD it, and how (Native node / generic HTTP / human work).
//! - `Auth`  — what the CLIENT must provide once (nothing / an API key / OAuth consent).
//!
//! The catalog is intentionally curated (not the full 400+ n8n nodes): it pins the
//! tier of the common SMB services so the LLM can't over-promise the ones that
//! matter. Long-tail services are handled by the rules in `designer_brief()`.

use std::fmt;

/// How an integration is delivered. Drives whether the designer may present it as
/// turnkey, and feeds the 3-tier offer (Instant / Assisted / Managed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// A dedicated n8n node exists → built & deployed automatically (Instant).
    Native,
    /// No dedicated node, but a documented REST API → built via httpRequest;
    /// the credential is wired by hand once (Assisted).
    Http,
    /// No node and no clean public API (bespoke/internal) → human integration
    /// work (Managed). NEVER presented as automatic.
    Managed,
}

impl Tier {
    /// Short tag the designer must put next to each integration in the blueprint.
    pub fn tag(self) -> &'static str {
        match self {
            Tier::Native => "Native",
            Tier::Http => "HTTP",
            Tier::Managed => "Managed",
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

/// What the client must provide once for the integration to work. Surfaced as a
/// prerequisite ("Points de vigilance") so the quote is honest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auth {
    /// No credential (public data, internal logic, schedule/webhook trigger).
    None,
    /// A token / API key the client pastes once (auto-wired thereafter).
    ApiKey,
    /// OAuth2 — the client must click "authorize" (cannot be fully automated).
    OAuth2,
}

impl Auth {
    pub fn prerequisite(self) -> Option<&'static str> {
        match self {
            Auth::None => None,
            Auth::ApiKey => Some("clé API à fournir"),
            Auth::OAuth2 => Some("connexion OAuth à autoriser"),
        }
    }
}

/// One deliverable integration.
#[derive(Debug, Clone, Copy)]
pub struct Capability {
    /// Canonical display name.
    pub service: &'static str,
    /// Lowercase aliases for matching a name from the need/research.
    pub aliases: &'static [&'static str],
    pub tier: Tier,
    pub auth: Auth,
}

/// The curated catalog. Grow it as we confirm coverage; under-listing only makes us
/// under-promise (safe), never over-promise.
pub const CATALOG: &[Capability] = &[
    // ── Messaging / email ──────────────────────────────────────────────────────
    Capability { service: "Slack",            aliases: &["slack"],                         tier: Tier::Native, auth: Auth::OAuth2 },
    Capability { service: "Gmail",            aliases: &["gmail", "google mail"],          tier: Tier::Native, auth: Auth::OAuth2 },
    Capability { service: "Microsoft Outlook",aliases: &["outlook", "office 365 mail"],    tier: Tier::Native, auth: Auth::OAuth2 },
    Capability { service: "Telegram",         aliases: &["telegram"],                      tier: Tier::Native, auth: Auth::ApiKey },
    Capability { service: "Discord",          aliases: &["discord"],                       tier: Tier::Native, auth: Auth::ApiKey },
    Capability { service: "Twilio",           aliases: &["twilio", "sms"],                 tier: Tier::Native, auth: Auth::ApiKey },
    Capability { service: "SendGrid",         aliases: &["sendgrid"],                      tier: Tier::Native, auth: Auth::ApiKey },
    Capability { service: "Mailchimp",        aliases: &["mailchimp"],                     tier: Tier::Native, auth: Auth::OAuth2 },
    // ── CRM / sales ─────────────────────────────────────────────────────────────
    Capability { service: "HubSpot",          aliases: &["hubspot"],                       tier: Tier::Native, auth: Auth::OAuth2 },
    Capability { service: "Pipedrive",        aliases: &["pipedrive"],                     tier: Tier::Native, auth: Auth::ApiKey },
    Capability { service: "Salesforce",       aliases: &["salesforce"],                    tier: Tier::Native, auth: Auth::OAuth2 },
    Capability { service: "Zoho CRM",         aliases: &["zoho", "zoho crm"],              tier: Tier::Native, auth: Auth::OAuth2 },
    // ── Productivity / data ─────────────────────────────────────────────────────
    Capability { service: "Google Sheets",    aliases: &["google sheets", "sheets", "gsheet"], tier: Tier::Native, auth: Auth::OAuth2 },
    Capability { service: "Google Drive",     aliases: &["google drive", "gdrive", "drive"],   tier: Tier::Native, auth: Auth::OAuth2 },
    Capability { service: "Google Calendar",  aliases: &["google calendar", "gcal", "calendar"], tier: Tier::Native, auth: Auth::OAuth2 },
    Capability { service: "Notion",           aliases: &["notion"],                        tier: Tier::Native, auth: Auth::ApiKey },
    Capability { service: "Airtable",         aliases: &["airtable"],                      tier: Tier::Native, auth: Auth::ApiKey },
    Capability { service: "Trello",           aliases: &["trello"],                        tier: Tier::Native, auth: Auth::ApiKey },
    Capability { service: "Asana",            aliases: &["asana"],                         tier: Tier::Native, auth: Auth::OAuth2 },
    // ── Commerce / billing ──────────────────────────────────────────────────────
    Capability { service: "Stripe",           aliases: &["stripe"],                        tier: Tier::Native, auth: Auth::ApiKey },
    Capability { service: "Shopify",          aliases: &["shopify"],                       tier: Tier::Native, auth: Auth::OAuth2 },
    Capability { service: "WooCommerce",      aliases: &["woocommerce", "woo"],            tier: Tier::Native, auth: Auth::ApiKey },
    // ── Databases ───────────────────────────────────────────────────────────────
    Capability { service: "PostgreSQL",       aliases: &["postgres", "postgresql"],        tier: Tier::Native, auth: Auth::ApiKey },
    Capability { service: "MySQL",            aliases: &["mysql", "mariadb"],              tier: Tier::Native, auth: Auth::ApiKey },
    Capability { service: "MongoDB",          aliases: &["mongodb", "mongo"],              tier: Tier::Native, auth: Auth::ApiKey },
    // ── AI ──────────────────────────────────────────────────────────────────────
    Capability { service: "OpenAI",           aliases: &["openai", "gpt", "chatgpt"],      tier: Tier::Native, auth: Auth::ApiKey },
    Capability { service: "Anthropic Claude", aliases: &["anthropic", "claude"],           tier: Tier::Native, auth: Auth::ApiKey },
    // ── Social / content ────────────────────────────────────────────────────────
    Capability { service: "X / Twitter",      aliases: &["twitter", "x.com", "tweet"],     tier: Tier::Native, auth: Auth::OAuth2 },
    Capability { service: "YouTube",          aliases: &["youtube"],                       tier: Tier::Native, auth: Auth::OAuth2 },
    Capability { service: "LinkedIn",         aliases: &["linkedin"],                      tier: Tier::Native, auth: Auth::OAuth2 },
    // ── Generic triggers / data sources (no credential) ─────────────────────────
    Capability { service: "Webhook (HTTP entrant)", aliases: &["webhook", "http trigger"], tier: Tier::Native, auth: Auth::None },
    Capability { service: "Planification (cron)",    aliases: &["schedule", "cron", "planification"], tier: Tier::Native, auth: Auth::None },
    Capability { service: "Flux RSS",         aliases: &["rss", "rss feed"],               tier: Tier::Native, auth: Auth::None },
];

/// Case-insensitive substring match of `name` against catalog aliases.
/// Returns the matched capability if `name` mentions a cataloged service.
pub fn classify(name: &str) -> Option<&'static Capability> {
    let n = name.to_lowercase();
    CATALOG.iter().find(|c| c.aliases.iter().any(|a| n.contains(a)))
}

/// The catalog brief injected into the designer's system prompt. Pins the tier
/// vocabulary, lists the confidently-deliverable services, and forces honest
/// per-integration tagging + the rule for long-tail/bespoke services.
pub fn designer_brief() -> String {
    let native: Vec<&str> = CATALOG.iter()
        .filter(|c| c.tier == Tier::Native).map(|c| c.service).collect();

    format!(
        "=== CATALOGUE DE CAPACITÉS — ce que pointe.dev sait RÉELLEMENT livrer ===\n\
        Trois niveaux de livraison. Tu DOIS taguer chaque intégration du design avec le sien:\n\
        - [Native] : nœud n8n dédié → construit et déployé automatiquement. Services confirmés:\n  {native}.\n\
        - [HTTP] : pas de nœud dédié mais une API REST publique documentée → construit via httpRequest, \
        identifiants câblés à la main une fois. Tout SaaS connu avec une API publique entre ici.\n\
        - [Managed] : système sur mesure/interne, CRM/ERP maison, ou service sans API publique → \
        intégration réalisée par notre équipe (PAS automatique).\n\
        \n\
        RÈGLES DE PROMESSE (honnêteté par construction):\n\
        1. Ne présente JAMAIS une intégration [Managed] comme clé-en-main: signale-la explicitement \
        comme nécessitant notre mise en place.\n\
        2. Un service que le client nomme et qui n'est pas [Native] ci-dessus: s'il a une API publique \
        connue → [HTTP]; si c'est un outil interne/maison ou au périmètre flou → [Managed].\n\
        3. En cas de doute sur l'existence d'un nœud dédié, vérifie via search_nodes avant de taguer \
        [Native]. Sous-promettre ([HTTP] au lieu de [Native]) est acceptable; sur-promettre ne l'est pas.\n\
        \n\
        Après les étapes, AJOUTE une ligne:\n\
        Livraison par intégration: <Service> [Tier] (<prérequis client: clé API / connexion OAuth / aucun>), …\n\
        — une entrée par service externe distinct. Les prérequis OAuth/clé API doivent aussi apparaître \
        dans 'Points de vigilance'.",
        native = native.join(", "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_matches_known_services_case_insensitively() {
        assert_eq!(classify("on push to Slack").unwrap().service, "Slack");
        assert_eq!(classify("HUBSPOT crm").unwrap().tier, Tier::Native);
        assert_eq!(classify("send via gmail").unwrap().auth, Auth::OAuth2);
        assert_eq!(classify("a Postgres table").unwrap().service, "PostgreSQL");
    }

    #[test]
    fn classify_returns_none_for_uncataloged() {
        assert!(classify("our internal billing mainframe").is_none());
        assert!(classify("SAP S/4HANA").is_none());
    }

    #[test]
    fn every_catalog_entry_has_at_least_one_alias() {
        for c in CATALOG {
            assert!(!c.aliases.is_empty(), "{} has no aliases", c.service);
        }
    }

    #[test]
    fn designer_brief_lists_native_services_and_tier_rules() {
        let brief = designer_brief();
        assert!(brief.contains("Slack") && brief.contains("HubSpot"));
        assert!(brief.contains("[Native]") && brief.contains("[HTTP]") && brief.contains("[Managed]"));
        assert!(brief.contains("Livraison par intégration"));
    }

    #[test]
    fn auth_prerequisite_wording() {
        assert_eq!(Auth::None.prerequisite(), None);
        assert!(Auth::OAuth2.prerequisite().unwrap().contains("OAuth"));
        assert!(Auth::ApiKey.prerequisite().unwrap().contains("clé API"));
    }
}

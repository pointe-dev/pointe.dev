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
    /// n8n credential type id for in-app provisioning (e.g. "notionApi"). `None` =
    /// provisioning not wired yet (the credentials engine reports it as manual).
    /// Only consulted for `Auth::ApiKey` services in v1.
    pub cred_type: Option<&'static str>,
}

/// The curated catalog. Grow it as we confirm coverage; under-listing only makes us
/// under-promise (safe), never over-promise.
pub const CATALOG: &[Capability] = &[
    // ── Messaging / email ──────────────────────────────────────────────────────
    Capability { service: "Slack",            aliases: &["slack"],                         tier: Tier::Native, auth: Auth::OAuth2, cred_type: Some("slackOAuth2Api") },
    Capability { service: "Gmail",            aliases: &["gmail", "google mail"],          tier: Tier::Native, auth: Auth::OAuth2, cred_type: Some("gmailOAuth2") },
    Capability { service: "Microsoft Outlook",aliases: &["outlook", "office 365 mail"],    tier: Tier::Native, auth: Auth::OAuth2, cred_type: Some("microsoftOutlookOAuth2Api") },
    Capability { service: "Telegram",         aliases: &["telegram"],                      tier: Tier::Native, auth: Auth::ApiKey, cred_type: Some("telegramApi") },
    Capability { service: "Discord",          aliases: &["discord"],                       tier: Tier::Native, auth: Auth::ApiKey, cred_type: None },
    Capability { service: "Twilio",           aliases: &["twilio", "sms"],                 tier: Tier::Native, auth: Auth::ApiKey, cred_type: Some("twilioApi") },
    Capability { service: "SendGrid",         aliases: &["sendgrid"],                      tier: Tier::Native, auth: Auth::ApiKey, cred_type: Some("sendGridApi") },
    Capability { service: "Mailchimp",        aliases: &["mailchimp"],                     tier: Tier::Native, auth: Auth::OAuth2, cred_type: Some("mailchimpOAuth2Api") },
    // ── CRM / sales ─────────────────────────────────────────────────────────────
    Capability { service: "HubSpot",          aliases: &["hubspot"],                       tier: Tier::Native, auth: Auth::OAuth2, cred_type: Some("hubspotOAuth2Api") },
    Capability { service: "Pipedrive",        aliases: &["pipedrive"],                     tier: Tier::Native, auth: Auth::ApiKey, cred_type: Some("pipedriveApi") },
    Capability { service: "Salesforce",       aliases: &["salesforce"],                    tier: Tier::Native, auth: Auth::OAuth2, cred_type: Some("salesforceOAuth2Api") },
    Capability { service: "Zoho CRM",         aliases: &["zoho", "zoho crm"],              tier: Tier::Native, auth: Auth::OAuth2, cred_type: Some("zohoOAuth2Api") },
    // ── Productivity / data ─────────────────────────────────────────────────────
    Capability { service: "Google Sheets",    aliases: &["google sheets", "sheets", "gsheet"], tier: Tier::Native, auth: Auth::OAuth2, cred_type: Some("googleSheetsOAuth2Api") },
    Capability { service: "Google Drive",     aliases: &["google drive", "gdrive", "drive"],   tier: Tier::Native, auth: Auth::OAuth2, cred_type: Some("googleDriveOAuth2Api") },
    Capability { service: "Google Calendar",  aliases: &["google calendar", "gcal", "calendar"], tier: Tier::Native, auth: Auth::OAuth2, cred_type: Some("googleCalendarOAuth2Api") },
    Capability { service: "Notion",           aliases: &["notion"],                        tier: Tier::Native, auth: Auth::ApiKey, cred_type: Some("notionApi") },
    Capability { service: "Airtable",         aliases: &["airtable"],                      tier: Tier::Native, auth: Auth::ApiKey, cred_type: Some("airtableTokenApi") },
    Capability { service: "Trello",           aliases: &["trello"],                        tier: Tier::Native, auth: Auth::ApiKey, cred_type: Some("trelloApi") },
    Capability { service: "Asana",            aliases: &["asana"],                         tier: Tier::Native, auth: Auth::OAuth2, cred_type: Some("asanaOAuth2Api") },
    // ── Commerce / billing ──────────────────────────────────────────────────────
    Capability { service: "Stripe",           aliases: &["stripe"],                        tier: Tier::Native, auth: Auth::ApiKey, cred_type: Some("stripeApi") },
    Capability { service: "Shopify",          aliases: &["shopify"],                       tier: Tier::Native, auth: Auth::OAuth2, cred_type: Some("shopifyOAuth2Api") },
    Capability { service: "WooCommerce",      aliases: &["woocommerce", "woo"],            tier: Tier::Native, auth: Auth::ApiKey, cred_type: Some("wooCommerceApi") },
    // ── Databases ───────────────────────────────────────────────────────────────
    Capability { service: "PostgreSQL",       aliases: &["postgres", "postgresql"],        tier: Tier::Native, auth: Auth::ApiKey, cred_type: Some("postgres") },
    Capability { service: "MySQL",            aliases: &["mysql", "mariadb"],              tier: Tier::Native, auth: Auth::ApiKey, cred_type: Some("mySql") },
    Capability { service: "MongoDB",          aliases: &["mongodb", "mongo"],              tier: Tier::Native, auth: Auth::ApiKey, cred_type: None },
    // ── AI ──────────────────────────────────────────────────────────────────────
    Capability { service: "OpenAI",           aliases: &["openai", "gpt", "chatgpt"],      tier: Tier::Native, auth: Auth::ApiKey, cred_type: Some("openAiApi") },
    Capability { service: "Anthropic Claude", aliases: &["anthropic", "claude"],           tier: Tier::Native, auth: Auth::ApiKey, cred_type: Some("anthropicApi") },
    // ── Social / content ────────────────────────────────────────────────────────
    Capability { service: "X / Twitter",      aliases: &["twitter", "x.com", "tweet"],     tier: Tier::Native, auth: Auth::OAuth2, cred_type: Some("twitterOAuth2Api") },
    Capability { service: "YouTube",          aliases: &["youtube"],                       tier: Tier::Native, auth: Auth::OAuth2, cred_type: Some("youTubeOAuth2Api") },
    Capability { service: "LinkedIn",         aliases: &["linkedin"],                      tier: Tier::Native, auth: Auth::OAuth2, cred_type: Some("linkedInOAuth2Api") },
    // ── Generic triggers / data sources (no credential) ─────────────────────────
    Capability { service: "Webhook (HTTP entrant)", aliases: &["webhook", "http trigger"], tier: Tier::Native, auth: Auth::None, cred_type: None },
    Capability { service: "Planification (cron)",    aliases: &["schedule", "cron", "planification"], tier: Tier::Native, auth: Auth::None, cred_type: None },
    Capability { service: "Flux RSS",         aliases: &["rss", "rss feed"],               tier: Tier::Native, auth: Auth::None, cred_type: None },
];

/// Case-insensitive substring match of `name` against catalog aliases.
/// Returns the matched capability if `name` mentions a cataloged service.
pub fn classify(name: &str) -> Option<&'static Capability> {
    let n = name.to_lowercase();
    CATALOG.iter().find(|c| c.aliases.iter().any(|a| n.contains(a)))
}

/// One integration the client must set up, classified deterministically by the
/// catalog (we do NOT trust the LLM's own tags here — the catalog is the source of
/// truth). Drives the post-payment delivery checklist (c.1).
#[derive(Debug, Clone)]
pub struct DeliveryItem {
    /// Canonical service name (catalog) or the raw token for an uncataloged one.
    pub service: String,
    pub tier: Tier,
    pub auth: Auth,
    /// n8n credential type for in-app provisioning, if wired.
    pub cred_type: Option<&'static str>,
}

/// Build the delivery plan from a design blueprint: parse its "Blocs clés:" line
/// (the distinct services), then classify each via the catalog. Uncataloged →
/// Managed (human setup). De-duplicated, order preserved. Returns empty if no
/// "Blocs clés" line is present.
pub fn delivery_plan(design_summary: &str) -> Vec<DeliveryItem> {
    let line = design_summary.lines().find(|l| {
        let t = l.trim_start().to_lowercase();
        t.starts_with("blocs cl") || t.starts_with("blocs-cl") || t.starts_with("key blocks")
    });
    let Some(line) = line else { return Vec::new() };
    let after = line.splitn(2, ':').nth(1).unwrap_or("");

    let mut out: Vec<DeliveryItem> = Vec::new();
    for raw in after.split(',') {
        // strip any "[Tier]" tag or "(...)" the LLM may have appended
        let token = raw.split('[').next().unwrap_or(raw)
            .split('(').next().unwrap_or(raw)
            .trim();
        if token.is_empty() { continue; }
        let item = match classify(token) {
            Some(c) => DeliveryItem { service: c.service.to_string(), tier: c.tier, auth: c.auth, cred_type: c.cred_type },
            None => DeliveryItem { service: token.to_string(), tier: Tier::Managed, auth: Auth::None, cred_type: None },
        };
        if !out.iter().any(|d| d.service.eq_ignore_ascii_case(&item.service)) {
            out.push(item);
        }
    }
    out
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
    fn delivery_plan_parses_and_classifies_blocs_cles() {
        let design = "1. Trigger — Webhook\n\
            2. Save — CRM maison\n\
            Blocs clés: Webhook, CRM maison, Gmail, Notion\n\
            Points de vigilance: aucun";
        let plan = delivery_plan(design);
        let by = |s: &str| plan.iter().find(|d| d.service == s).cloned();
        assert_eq!(by("Gmail").unwrap().auth, Auth::OAuth2);
        assert_eq!(by("Notion").unwrap().cred_type, Some("notionApi"));
        // uncataloged bespoke CRM → Managed
        assert_eq!(plan.iter().find(|d| d.service.contains("CRM")).unwrap().tier, Tier::Managed);
    }

    #[test]
    fn delivery_plan_dedups_and_strips_tags() {
        let plan = delivery_plan("Blocs clés: Slack [Native] (OAuth), Slack, slack");
        assert_eq!(plan.len(), 1, "Slack must appear once");
        assert_eq!(plan[0].service, "Slack");
    }

    #[test]
    fn delivery_plan_empty_without_blocs_line() {
        assert!(delivery_plan("just some steps, no key blocks line").is_empty());
    }

    #[test]
    fn auth_prerequisite_wording() {
        assert_eq!(Auth::None.prerequisite(), None);
        assert!(Auth::OAuth2.prerequisite().unwrap().contains("OAuth"));
        assert!(Auth::ApiKey.prerequisite().unwrap().contains("clé API"));
    }
}

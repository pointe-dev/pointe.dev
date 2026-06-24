//! Legal pages (trilingue FR/EN/DE) — micro-entreprise, droit français.
//!
//! Four documents selected by `LegalDoc`: mentions légales (LCEN), politique de
//! confidentialité (RGPD), CGV/CGU, politique cookies. The French version is the
//! legally binding one; EN/DE are courtesy translations.
//!
//! ⚠️ Placeholders in SQUARE BRACKETS ([NOM], [ADRESSE], [EMAIL]) must be filled in
//! by the owner before publication — they are legally required (LCEN) and are not
//! invented here. The SIRET and host are real.
//!
//! Copy lives inline (long-form legal text doesn't fit the flat i18n match). Each
//! section is a (heading, body) pair; bodies may contain multiple paragraphs split
//! on "\n\n".

use leptos::*;

use crate::i18n::Lang;

/// Which legal document to render. Maps 1:1 to a public URL path.
#[derive(Clone, Copy, PartialEq)]
pub enum LegalDoc {
    Mentions,       // /mentions-legales
    Privacy,        // /confidentialite
    Terms,          // /cgv
    Cookies,        // /cookies
}

// ── Real identifiers (safe to publish) ──────────────────────────────────────────
const SIRET: &str = "10666720170017";
const HOST_FR: &str = "Le site est hébergé par Hetzner Online GmbH (Industriestr. 25, 91710 Gunzenhausen, Allemagne) ; la distribution et la protection (CDN/proxy/DNS) sont assurées par Cloudflare, Inc. (101 Townsend Street, San Francisco, CA 94107, États-Unis).";
const HOST_EN: &str = "The site is hosted by Hetzner Online GmbH (Industriestr. 25, 91710 Gunzenhausen, Germany); delivery and protection (CDN/proxy/DNS) are provided by Cloudflare, Inc. (101 Townsend Street, San Francisco, CA 94107, USA).";
const HOST_DE: &str = "Die Website wird von der Hetzner Online GmbH (Industriestr. 25, 91710 Gunzenhausen, Deutschland) gehostet; Auslieferung und Schutz (CDN/Proxy/DNS) erfolgen über Cloudflare, Inc. (101 Townsend Street, San Francisco, CA 94107, USA).";

/// (heading, body) pairs for a given doc + language.
fn content(doc: LegalDoc, lang: Lang) -> (&'static str, Vec<(&'static str, String)>) {
    match (doc, lang) {
        // ── Mentions légales ────────────────────────────────────────────────
        (LegalDoc::Mentions, Lang::Fr) => ("Mentions légales", vec![
            ("Éditeur du site", format!(
                "Le présent site est édité par [NOM], entrepreneur individuel (micro-entreprise).\n\n\
                 SIRET : {SIRET}\n\
                 Adresse : [ADRESSE À COMPLÉTER]\n\
                 Contact : [EMAIL]\n\
                 TVA non applicable, article 293 B du Code général des impôts.")),
            ("Directeur de la publication", "[NOM], en sa qualité d'éditeur.".into()),
            ("Hébergement", HOST_FR.into()),
            ("Propriété intellectuelle",
                "L'ensemble des contenus du site (textes, marques, logo, éléments graphiques) est protégé par le droit de la propriété intellectuelle. Toute reproduction sans autorisation est interdite.".into()),
            ("Responsabilité",
                "L'éditeur s'efforce d'assurer l'exactitude des informations diffusées mais ne saurait être tenu responsable des erreurs ou d'une indisponibilité du service.".into()),
        ]),
        (LegalDoc::Mentions, Lang::En) => ("Legal notice", vec![
            ("Publisher", format!(
                "This website is published by [NAME], sole trader (French micro-entreprise).\n\n\
                 SIRET: {SIRET}\n\
                 Address: [ADDRESS TO BE COMPLETED]\n\
                 Contact: [EMAIL]\n\
                 VAT not applicable, Article 293 B of the French General Tax Code.")),
            ("Publication director", "[NAME], as publisher.".into()),
            ("Hosting", HOST_EN.into()),
            ("Intellectual property",
                "All site content (text, trademarks, logo, graphics) is protected by intellectual-property law. Reproduction without permission is prohibited.".into()),
            ("Liability",
                "The publisher strives to keep information accurate but cannot be held liable for errors or service unavailability.".into()),
        ]),
        (LegalDoc::Mentions, Lang::De) => ("Impressum", vec![
            ("Herausgeber", format!(
                "Diese Website wird herausgegeben von [NAME], Einzelunternehmer (französische micro-entreprise).\n\n\
                 SIRET: {SIRET}\n\
                 Adresse: [ADRESSE NOCH EINZUTRAGEN]\n\
                 Kontakt: [EMAIL]\n\
                 Umsatzsteuer nicht anwendbar, Artikel 293 B des französischen Steuergesetzbuchs.")),
            ("Verantwortlich für den Inhalt", "[NAME], als Herausgeber.".into()),
            ("Hosting", HOST_DE.into()),
            ("Urheberrecht",
                "Sämtliche Inhalte der Website (Texte, Marken, Logo, Grafiken) sind urheberrechtlich geschützt. Eine Vervielfältigung ohne Genehmigung ist untersagt.".into()),
            ("Haftung",
                "Der Herausgeber bemüht sich um die Richtigkeit der Informationen, haftet jedoch nicht für Fehler oder eine Nichtverfügbarkeit des Dienstes.".into()),
        ]),

        // ── Politique de confidentialité (RGPD) ──────────────────────────────
        (LegalDoc::Privacy, Lang::Fr) => ("Politique de confidentialité", vec![
            ("Responsable du traitement", format!(
                "[NOM], entrepreneur individuel (SIRET {SIRET}), est responsable du traitement des données collectées via ce site. Contact : [EMAIL].")),
            ("Données collectées",
                "Nous collectons : votre adresse email (lorsque vous la saisissez pour accéder à l'agent), une empreinte technique de navigateur (anti-abus), le contenu de vos échanges avec l'agent, et — en cas de paiement — les données de transaction traitées par notre prestataire Stripe (nous ne stockons jamais votre numéro de carte).".into()),
            ("Finalités",
                "Fourniture du service de conversation et de conception d'automatisations, gestion des crédits et des paiements, prévention des abus, et communication avec vous au sujet de votre projet.".into()),
            ("Base légale",
                "Exécution de mesures précontractuelles et du contrat (article 6.1.b RGPD), intérêt légitime pour la prévention des abus (article 6.1.f), et votre consentement pour les cookies non essentiels (article 6.1.a).".into()),
            ("Sous-traitants",
                "Stripe (paiement), Resend (envoi d'emails de confirmation), Cloudflare (protection/diffusion), Hetzner (hébergement), et un fournisseur de modèles d'IA pour les réponses de l'agent. Certains peuvent traiter des données hors UE, avec les garanties appropriées.".into()),
            ("Durée de conservation",
                "Les données sont conservées le temps nécessaire à la fourniture du service puis archivées ou supprimées conformément aux obligations légales.".into()),
            ("Vos droits",
                "Vous disposez d'un droit d'accès, de rectification, d'effacement, de limitation, d'opposition et de portabilité. Pour les exercer : [EMAIL]. Vous pouvez aussi saisir la CNIL (www.cnil.fr).".into()),
        ]),
        (LegalDoc::Privacy, Lang::En) => ("Privacy policy", vec![
            ("Data controller", format!(
                "[NAME], sole trader (SIRET {SIRET}), is the controller of data collected via this site. Contact: [EMAIL].")),
            ("Data we collect",
                "We collect: your email address (when you enter it to access the agent), a technical browser fingerprint (anti-abuse), the content of your exchanges with the agent, and — for payments — transaction data processed by our provider Stripe (we never store your card number).".into()),
            ("Purposes",
                "Providing the conversation and automation-design service, managing credits and payments, preventing abuse, and communicating with you about your project.".into()),
            ("Legal basis",
                "Pre-contractual and contractual performance (Art. 6.1.b GDPR), legitimate interest in abuse prevention (Art. 6.1.f), and your consent for non-essential cookies (Art. 6.1.a).".into()),
            ("Processors",
                "Stripe (payments), Resend (confirmation emails), Cloudflare (protection/delivery), Hetzner (hosting), and an AI model provider for the agent's replies. Some may process data outside the EU, with appropriate safeguards.".into()),
            ("Retention",
                "Data is kept for as long as needed to provide the service, then archived or deleted in line with legal obligations.".into()),
            ("Your rights",
                "You have rights of access, rectification, erasure, restriction, objection and portability. To exercise them: [EMAIL]. You may also lodge a complaint with the CNIL (www.cnil.fr).".into()),
        ]),
        (LegalDoc::Privacy, Lang::De) => ("Datenschutzerklärung", vec![
            ("Verantwortlicher", format!(
                "[NAME], Einzelunternehmer (SIRET {SIRET}), ist für die über diese Website erhobenen Daten verantwortlich. Kontakt: [EMAIL].")),
            ("Erhobene Daten",
                "Wir erheben: Ihre E-Mail-Adresse (bei Eingabe für den Zugang zum Agenten), einen technischen Browser-Fingerabdruck (Missbrauchsschutz), den Inhalt Ihrer Konversationen mit dem Agenten und — bei Zahlungen — von unserem Dienstleister Stripe verarbeitete Transaktionsdaten (wir speichern niemals Ihre Kartennummer).".into()),
            ("Zwecke",
                "Bereitstellung des Konversations- und Automatisierungsdienstes, Verwaltung von Guthaben und Zahlungen, Missbrauchsprävention und Kommunikation über Ihr Projekt.".into()),
            ("Rechtsgrundlage",
                "Vorvertragliche und vertragliche Erfüllung (Art. 6.1.b DSGVO), berechtigtes Interesse an der Missbrauchsprävention (Art. 6.1.f) und Ihre Einwilligung für nicht wesentliche Cookies (Art. 6.1.a).".into()),
            ("Auftragsverarbeiter",
                "Stripe (Zahlung), Resend (Bestätigungs-E-Mails), Cloudflare (Schutz/Auslieferung), Hetzner (Hosting) und ein KI-Modellanbieter für die Antworten des Agenten. Einige können Daten außerhalb der EU mit geeigneten Garantien verarbeiten.".into()),
            ("Speicherdauer",
                "Die Daten werden so lange gespeichert, wie es für die Erbringung des Dienstes erforderlich ist, und anschließend gemäß den gesetzlichen Pflichten archiviert oder gelöscht.".into()),
            ("Ihre Rechte",
                "Sie haben das Recht auf Auskunft, Berichtigung, Löschung, Einschränkung, Widerspruch und Übertragbarkeit. Zur Ausübung: [EMAIL]. Sie können sich auch an die CNIL (www.cnil.fr) wenden.".into()),
        ]),

        // ── CGV / CGU ────────────────────────────────────────────────────────
        (LegalDoc::Terms, Lang::Fr) => ("Conditions générales de vente et d'utilisation", vec![
            ("Objet",
                "Les présentes conditions régissent l'utilisation du service et la vente des prestations d'automatisation proposées sur ce site par [NOM] (micro-entreprise, SIRET ".to_string() + SIRET + ")."),
            ("Service et crédits",
                "L'accès à l'agent nécessite une adresse email. Des crédits de conversation gratuits sont offerts à l'inscription ; ils sont consommés à l'usage. Des crédits supplémentaires peuvent être achetés. Les crédits offerts mensuels d'un abonnement sont remis à zéro chaque mois ; les crédits achetés ne sont pas réinitialisés.".into()),
            ("Commande et paiement",
                "La conception et le devis sont réalisés en ligne. La construction et le déploiement d'une automatisation interviennent après paiement, traité de façon sécurisée par Stripe. Les prix sont indiqués en euros ; TVA non applicable (art. 293 B du CGI).".into()),
            ("Droit de rétractation",
                "Pour les prestations de services numériques exécutées immédiatement avec votre accord, le droit de rétractation peut ne pas s'appliquer une fois l'exécution commencée, conformément au Code de la consommation. Les crédits consommés ne sont pas remboursables.".into()),
            ("Responsabilité",
                "Le service est fourni en l'état. L'éditeur ne saurait être tenu responsable des dommages indirects. Sa responsabilité est limitée au montant payé pour la prestation concernée.".into()),
            ("Droit applicable",
                "Les présentes conditions sont régies par le droit français. Tout litige relève des tribunaux compétents français, après recherche d'une solution amiable.".into()),
        ]),
        (LegalDoc::Terms, Lang::En) => ("Terms of sale and use", vec![
            ("Purpose",
                "These terms govern use of the service and the sale of the automation services offered on this site by [NAME] (micro-entreprise, SIRET ".to_string() + SIRET + ")."),
            ("Service and credits",
                "Access to the agent requires an email address. Free conversation credits are granted at sign-up and consumed with use. Additional credits may be purchased. A subscription's monthly free credits reset each month; purchased credits do not reset.".into()),
            ("Order and payment",
                "Design and quoting happen online. Building and deploying an automation occur after payment, securely processed by Stripe. Prices are in euros; VAT not applicable (Art. 293 B French Tax Code).".into()),
            ("Right of withdrawal",
                "For digital services performed immediately with your agreement, the right of withdrawal may not apply once performance has begun, per French consumer law. Consumed credits are non-refundable.".into()),
            ("Liability",
                "The service is provided as is. The publisher is not liable for indirect damages; liability is limited to the amount paid for the relevant service.".into()),
            ("Governing law",
                "These terms are governed by French law. Disputes fall under the competent French courts, after seeking an amicable solution.".into()),
        ]),
        (LegalDoc::Terms, Lang::De) => ("Allgemeine Geschäfts- und Nutzungsbedingungen", vec![
            ("Gegenstand",
                "Diese Bedingungen regeln die Nutzung des Dienstes und den Verkauf der auf dieser Website angebotenen Automatisierungsleistungen durch [NAME] (micro-entreprise, SIRET ".to_string() + SIRET + ")."),
            ("Dienst und Guthaben",
                "Der Zugang zum Agenten erfordert eine E-Mail-Adresse. Bei der Anmeldung werden kostenlose Konversationsguthaben gewährt und bei Nutzung verbraucht. Zusätzliche Guthaben können erworben werden. Die monatlichen Freiguthaben eines Abonnements werden monatlich zurückgesetzt; gekaufte Guthaben werden nicht zurückgesetzt.".into()),
            ("Bestellung und Zahlung",
                "Design und Angebot erfolgen online. Aufbau und Deployment einer Automatisierung erfolgen nach Zahlung, sicher abgewickelt durch Stripe. Preise in Euro; Umsatzsteuer nicht anwendbar (Art. 293 B frz. Steuergesetzbuch).".into()),
            ("Widerrufsrecht",
                "Bei sofort mit Ihrer Zustimmung erbrachten digitalen Leistungen kann das Widerrufsrecht nach Beginn der Ausführung entfallen (frz. Verbraucherrecht). Verbrauchte Guthaben sind nicht erstattungsfähig.".into()),
            ("Haftung",
                "Der Dienst wird wie besehen bereitgestellt. Der Herausgeber haftet nicht für indirekte Schäden; die Haftung ist auf den für die betreffende Leistung gezahlten Betrag begrenzt.".into()),
            ("Anwendbares Recht",
                "Es gilt französisches Recht. Für Streitigkeiten sind die zuständigen französischen Gerichte zuständig, nach Versuch einer gütlichen Einigung.".into()),
        ]),

        // ── Politique cookies ────────────────────────────────────────────────
        (LegalDoc::Cookies, Lang::Fr) => ("Politique de gestion des cookies", vec![
            ("Qu'est-ce qu'un cookie",
                "Un cookie est un petit fichier déposé sur votre appareil lors de la visite d'un site. Il permet d'assurer le fonctionnement du service et, le cas échéant, de mesurer l'audience.".into()),
            ("Cookies utilisés",
                "Cookies strictement nécessaires : gestion de votre session et de l'accès au service (toujours actifs, sans consentement requis). Cookies de mesure/protection : déposés par Cloudflare pour la sécurité et la performance. Aucun cookie publicitaire tiers n'est utilisé.".into()),
            ("Votre consentement",
                "Lors de votre première visite, une bannière vous permet d'accepter ou de refuser les cookies non essentiels. Vous pouvez modifier votre choix à tout moment en effaçant les cookies de votre navigateur.".into()),
            ("Gestion",
                "Vous pouvez configurer votre navigateur pour refuser les cookies. Le refus des cookies strictement nécessaires peut empêcher le bon fonctionnement du service.".into()),
        ]),
        (LegalDoc::Cookies, Lang::En) => ("Cookie policy", vec![
            ("What is a cookie",
                "A cookie is a small file stored on your device when you visit a site. It keeps the service working and, where applicable, measures audience.".into()),
            ("Cookies we use",
                "Strictly necessary cookies: managing your session and access to the service (always on, no consent required). Measurement/protection cookies: set by Cloudflare for security and performance. No third-party advertising cookies are used.".into()),
            ("Your consent",
                "On your first visit, a banner lets you accept or refuse non-essential cookies. You can change your choice at any time by clearing your browser cookies.".into()),
            ("Managing cookies",
                "You can configure your browser to refuse cookies. Refusing strictly necessary cookies may prevent the service from working properly.".into()),
        ]),
        (LegalDoc::Cookies, Lang::De) => ("Cookie-Richtlinie", vec![
            ("Was ist ein Cookie",
                "Ein Cookie ist eine kleine Datei, die beim Besuch einer Website auf Ihrem Gerät gespeichert wird. Sie hält den Dienst funktionsfähig und misst gegebenenfalls die Reichweite.".into()),
            ("Verwendete Cookies",
                "Unbedingt erforderliche Cookies: Verwaltung Ihrer Sitzung und des Zugangs zum Dienst (immer aktiv, keine Einwilligung erforderlich). Mess-/Schutz-Cookies: von Cloudflare zur Sicherheit und Leistung gesetzt. Es werden keine Werbe-Cookies Dritter verwendet.".into()),
            ("Ihre Einwilligung",
                "Bei Ihrem ersten Besuch können Sie über ein Banner nicht wesentliche Cookies annehmen oder ablehnen. Sie können Ihre Wahl jederzeit ändern, indem Sie die Cookies Ihres Browsers löschen.".into()),
            ("Verwaltung",
                "Sie können Ihren Browser so einstellen, dass er Cookies ablehnt. Die Ablehnung unbedingt erforderlicher Cookies kann die Funktion des Dienstes beeinträchtigen.".into()),
        ]),
    }
}

/// "Last updated" line, localized.
fn updated_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Fr => "Dernière mise à jour : juin 2026",
        Lang::En => "Last updated: June 2026",
        Lang::De => "Zuletzt aktualisiert: Juni 2026",
    }
}

#[component]
pub fn Legal(doc: LegalDoc) -> impl IntoView {
    let lang = use_context::<RwSignal<Lang>>()
        .unwrap_or_else(|| create_rw_signal(Lang::Fr));

    // Re-render copy when the language switches.
    let body = move || {
        let (title, sections) = content(doc, lang.get());
        let secs = sections.into_iter().map(|(h, b)| {
            let paras = b.split("\n\n").map(|p| {
                let p = p.to_string();
                view! { <p class="mt-3 text-sm text-secondary leading-relaxed whitespace-pre-line">{p}</p> }
            }).collect_view();
            view! {
                <div class="mb-8">
                    <h2 class="text-xs font-semibold uppercase tracking-wider text-red-400 mb-1">{h}</h2>
                    {paras}
                </div>
            }
        }).collect_view();
        view! {
            <h1 class="text-3xl md:text-4xl font-bold text-gradient mb-2">{title}</h1>
            <p class="text-xs text-muted mb-10">{updated_label(lang.get())}</p>
            {secs}
        }
    };

    view! {
        <section class="max-w-3xl mx-auto px-6 py-16 md:py-24">
            {body}
        </section>
    }
}

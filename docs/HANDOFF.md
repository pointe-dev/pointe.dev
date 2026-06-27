# Handoff

## ⟳ Session 2026-06-26 — Bug A résolu + modèle de crédits COMPLET + légal/SEO/n8n

Tout ci-dessous est **mergé sur `main` + déployé** (dernier merge `4bbb5bb`). Working tree propre.

**✅ BUG A (paiement prod) — RÉSOLU.** C'était le **KYC Stripe**, jamais le code. KYC validé 2026-06-26 → `charges_enabled=true`, `card_payments=active`, création de session checkout LIVE testée OK (HTTP 200, `cs_live_…`). Encaissement opérationnel. La sonde `account_health` au boot (PR #27) est mergée (log honnête de `charges_enabled` au démarrage). **Reste optionnel : 1 vrai paiement E2E de preuve + remboursement.**

**✅ MODÈLE DE CRÉDITS — COMPLET** (funnel d'argent entier) :
- Gate **email obligatoire avant le 1er message** (capture → 5 $ offerts immédiats ; vérif lien repoussée avant paiement).
- Crédits en 2 poches : **offerts** (`gift`, reset mensuel) consommés avant **achetés** (`purchased`, persistants). Barème centimes ajustable (`sessions.rs`) : signup 500, message 10, research/design/pitch 50, top-up 1000.
- Débit par message (`handle_ai_chat`) + aux étapes pipeline pré-paiement (research/design/pitch ; build/deploy = post-paiement, non débités). Solde épuisé → `needs_credits`.
- **Top-up** ponctuel : `POST /api/credits/topup` (`create_credit_topup_checkout`, kind=topup).
- **Abonnement projet** (dernier morceau, merge `c161b4b`) : au paiement du pitch avec mensuel>0 → `create_subscription_checkout` (mode=subscription : setup one-time + mensuel recurring). Allocation mensuelle de crédits dérivée de `price_monthly` via `monthly_gift_for_price` (<50€→10$, 50–149€→25$, ≥150€→50$). Metadata kind=project_sub sur session+subscription → `invoice.paid` ré-applique l'alloc chaque mois.
- Webhook (`handlers/stripe.rs`) route par `metadata.kind` : topup→add_purchased_credits ; project_sub/invoice.paid→set_monthly_gift ; défaut→resume pipeline.
- Frontend `chat.rs` : invite email en amont, pastille « X,XX $ de crédits », bouton top-up.

**✅ PAGES LÉGALES** (4, trilingues FR/EN/DE, `pages/legal.rs`) : mentions-legales, confidentialite, cgv, cookies. Micro-entreprise, **vrais identifiants** : Comlan Amouwotor, raison sociale AMOUWOTOR COMLAN, SIREN 106672017, SIRET 10667201700014, 47 rue Vivienne 75002 Paris, contact@pointe.dev, TVA art.293B, hébergeur Hetzner+Cloudflare. Routes serve_index (+/faq), liens footer, sitemap. ⚠️ Relecture juridique CGV recommandée (textes générés, pas conseil juridique).

**✅ SEO** : canonical/og/hreflang sur **go.pointe.dev** (apex pointe.dev = futur portail séparé), JSON-LD statique Organization(logo)+WebSite+WebApplication, logo.png 512, sitemap/robots, assets copiés au Dockerfile, meta google-site-verification (Search Console **vérifié**). Détail → mémoire [[seo-prerender-nextstep]].

**✅ n8n** : MAJ sécurité 2.22.5→2.27.4 sans perte de données (SQLite volume `pointe_n8n_data`, clé chiffrement OK), tag épinglé + `mem_limit: 1g` (OOM réglé). Backup volume sur le serveur.

**✅ FIX NAV** : l'URL suit la navigation client (pushState + popstate ; fini le `/cgv` résiduel). Skeleton LCP = point rouge pulsant (au lieu du titre).

### 🚧 NEXT STEPS (au resume, ordre)
1. **Preuve E2E paiement réel** (action owner) : sur go.pointe.dev, chat → pitch AVEC mensuel → payer (vraie carte) → vérifier dans Stripe que l'abonnement est créé + crédits mensuels posés + pipeline repris → annuler/rembourser. Valide toute la chaîne argent.
2. **Actions owner** : redirection `contact@pointe.dev` (Cloudflare Email Routing) ; relecture juridique des CGV.
3. **Pré-rendu SEO** (chantier code, reco) : snapshot des routes publiques (/, /faq, légales) pour SEO de contenu. Détail/options/caveat Cloudflare → [[seo-prerender-nextstep]].
4. **Guardrails v2+** : vérif de propriété de domaine **DNS-TXT/.well-known** (durcissement #1 ; param `extra_hosts` de `owned_domains` déjà prêt), AUP, surfaçage espace client. → [[security-guardrails-watch]].
5. **Search Console** : soumettre le sitemap, surveiller l'indexation + l'apparition du logo (vérifier que Cloudflare ne challenge pas Googlebot — Bot Fight Mode off).

---

## 🐛 BUG A — DIAGNOSTIQUÉ (2026-06-22) : cause = compte Stripe non activé, PAS le code

**Cause racine confirmée (lecture read-only de l'API Stripe live via MCP, 2× indépendamment) :**
le compte live `acct_1Tc4xtE9iwCUGFAq` (Pointe.dev) a **`charges_enabled: false`** et
**toutes les capabilities `inactive`** (`card_payments: inactive`). Donc **aucun** checkout
ne peut aboutir — ce n'était ni la clé vide, ni `invoice_creation`, ni le code.

**Pourquoi :** la **vérification d'identité Stripe a échoué / est past-due**.
`requirements.disabled_reason = "requirements.past_due"`, et `currently_due` =
`individual.{first_name,last_name,phone,dob.day,dob.month,dob.year}`, chacun avec
l'erreur `verification_failed_keyed_identity` (« Insufficient records found for the person »).

**👉 ACTION OWNER (rien d'autre ne débloque l'encaissement) :** dans le Stripe Dashboard,
compléter la vérification d'identité. Deux alternatives proposées par Stripe
(`requirements.alternatives`) : fournir une **pièce d'identité** (`individual.verification.document`)
**ou** une **preuve de présence** (`individual.verification.proof_of_liveness`). Un compte
bancaire (Boursorama, last4 7181) est déjà rattaché. Une fois `charges_enabled: true`, le
funnel paiement marchera tel quel.

**Correction CODE apportée cette session (fail honestly early, dans la lignée de `18f7258`) :**
`StripeClient::account_health()` (GET `/v1/account`, read-only) + sonde au boot
(`main.rs`) : si `charges_enabled=false`, log `ERROR` explicite au démarrage au lieu du
`Stripe configured` trompeur — l'échec devient diagnosticable immédiatement plutôt que de
surgir en « Paiement momentanément indisponible » après le clic client. Tests unitaires
sur le parsing de `charges_enabled` ajoutés. **Ça ne rend pas les charges actives** (action
owner ci-dessus) — ça rend juste l'état honnête et visible.

---

## ⟳ Session 2026-06-22 — guardrails v2 mergé sur main

**Ce qui vient d'être mergé (`feat/guardrails-v2-ownership` → `main`, ce merge).**
Le push de `main` déclenche le déploiement prod via `.github/workflows/deploy.yml` —
**vérifier que le run passe** (onglet Actions, ou `docker pull …:sha-<sha>` côté serveur).
clingo tournera en vrai en prod (paquet `gringo` dans le Dockerfile) ; les 6 tests e2e
ASP `#[ignore]` ne tournent pas en CI sans clingo, mais la policy a été **validée contre
clingo 5.8.0 réel** cette session (7 scénarios).

Guardrails v2 = **architecture hybride à 2 couches, complémentaires** :
1. **Intention (amont, stade `qualify`)** — `agents::run_intent_check` : classifieur
   Haiku via tool call forcé → `{verdict, category, reason}`. Flague l'abus tiers /
   illégalité dans la PROSE avant de construire ; autorise le business normal même à
   volume s'il vise les propres clients/données du client. `Review` → `SavedForHuman` +
   notif owner. **Fail-open** (erreur LLM → Allow ; l'ASP backstoppe).
2. **Structure + ownership (aval, stade `Deploying`)** — ASP/clingo. v1 (flood/mass_post/
   scrape_loop) + v2 ownership (`unowned_bulk_post`, `unowned_flood`) : distingue
   « marteler SON domaine » de « un tiers ». **Preuve de propriété = domaine de l'email
   vérifié UNIQUEMENT** (webmails exclus ; le `client_n8n_url` tapé a été retiré comme
   preuve — sinon on pourrait « posséder » victim.com en le tapant).
Les deux remontent à l'admin via `stage_reason`. 23 nouveaux tests, suite verte, 0 clippy
sur le code neuf. Périmètre : ASP = sortant/abus ; OAuth = entrant/accès (voir oauth.md).
Détail → `docs/guardrails.md`.

### 🚧 NEXT STEPS (ordre, 2026-06-22)
1. **Vérifier le déploiement prod** de ce merge (Actions).
2. **🐛 BUG A — paiement prod KO (clé LIVE) — TOUJOURS OUVERT, bloqueur #1 encaissement.**
   Pas rediagnostiqué cette session. `Stripe configured` OK au boot mais aucun
   `create_checkout` dans les logs depuis le deploy → besoin d'une REPRO fraîche pour
   capturer `[stripe] checkout failed: …` (corps d'erreur Stripe exact, `handlers/stripe.rs:66`).
   Soit clic browser sur go.pointe.dev pendant qu'on lit les logs, soit repro directe de
   l'API Stripe live depuis le serveur (session non payée = 0€). **Suspect #1 :**
   `invoice_creation[enabled]=true` (`stripe.rs:66`) qui exige les réglages facturation
   du compte live complétés. **Suspect #2 :** compte live pas pleinement activé.
3. **⚠️ Mensuel récurrent non souscrit** (trouvé à l'audit du 2026-06-20) : le checkout
   met le mensuel en line-item PONCTUEL « 1er mois » (`mode=payment`), aucun
   `create_subscription`. À trancher avant tout « tiers d'exécutions ».
4. **Guardrails v2+ restant** : vraie **vérif de propriété de domaine DNS-TXT / .well-known**
   (le durcissement le + important ; param `extra_hosts` de `owned_domains` déjà prêt à
   recevoir des domaines prouvés), AUP/ToS au paiement, surfaçage espace client, élargir
   les classes ASP. Voir mémoire « Security & Guardrails Watch ».
5. **Env OAuth** (action owner) : poser `GOOGLE_OAUTH_CLIENT_ID/SECRET` etc.

---

## ⟳ Re-audit code — 2026-06-20

Audit du code réel cette session (pas de nouveau déploiement). Corrige plusieurs
notes de mémoire qui retardaient. **Source de vérité = ce fichier.**

**Vérifié au boot/CI :** déploiement `b2a641d` **passé** (run 2026-06-19 07:51,
success) ; `Stripe configured` présent dans les logs prod (clé live chargée).

**Confirmé FAIT dans le code (notes antérieures périmées) :**
- **Re-spawn des pipelines in-flight au boot** — `PipelineStage::is_resumable()`
  (exclut terminaux + AwaitingPayment), `resumable_ids()`, boucle de re-spawn
  `main.rs:710-721`, tests « building must be resumable / AwaitingPayment skipped ».
  Le « gap résiduel » (pipeline mid-flight non repris au restart) est **résolu**.
- **Guardrails v1 câblés** au stade `Deploying` (`pipeline.rs:790-809`) : `evaluate()`
  AVANT `run_deploy` → si pas `allows_auto_deploy(fail_closed())` → `SavedForHuman`
  + `notify_owner_failure`. Fail-open par défaut (`GUARDRAILS_FAIL_CLOSED`).
  Classes : flood / mass_post / scrape_loop. **v2 = pas commencé.**
- **Persistence** complète (pitch/sessions/pipelines, write-through + hydrate au boot).
  Migrations embarquées en code (`run_migrations`), pas de répertoire `migrations/`.
- Backend = **0 TODO/FIXME/unwrap/panic** (base propre).

**Nouveau point à clarifier — le mensuel récurrent n'est probablement PAS souscrit :**
le checkout (`stripe.rs:44-67`) met le mensuel en **line-item ponctuel** « 1er mois »
en `mode=payment` ; le commentaire dit « recurring subscription handled separately
after payment » mais **aucun `create_subscription` n'existe**. À trancher AVANT le
« tiers d'exécutions » (qui suppose un récurrent réel).

**Bug A toujours OUVERT mais NON reproduit** depuis le deploy `b2a641d` (aucun
`create_checkout` dans les logs) → il faut une repro fraîche pour capturer
`[stripe] checkout failed: …`. Suspect #1 : `invoice_creation[enabled]=true`
(`stripe.rs:66`) qui exige les réglages de facturation du compte live complétés.

**Décision owner 2026-06-20 :** Hero 3D **abandonné** (branche `feat/hero-3d-ballet`
à supprimer).

**Next steps révisés (ordre) :** 1) reproduire + fixer **Bug A** ; 2) clarifier le
**mensuel récurrent** ; 3) **env OAuth** (action owner) ; 4) **guardrails v2**
(ownership/intent/AUP).

---

# Handoff — 2026-06-19

Reprise après changement de machine. Tout le travail ci-dessous est **déjà sur
`main`** (mergé + poussé, commit de merge `b2a641d`). Working tree propre au moment
du handoff. Le push `main` déclenche le déploiement prod via `.github/workflows/deploy.yml`
(pull + restart, pas de rebuild) — **vérifier que le déploiement est passé** (onglet
Actions du repo, ou `docker pull …:sha-<sha>` côté serveur).

## Ce qui vient de partir en prod (session 2026-06-18/19)

1. **OAuth2 délégué à n8n** — bouton « Autoriser la connexion » sur la checklist de
   livraison. Clés d'app résolues **côté serveur** par provider (jamais dans le WASM).
   Tout le catalogue OAuth2 est câblé. Détail → [oauth.md](oauth.md).
2. **FAQ client trilingue** `/faq` (FR/EN/DE), lien footer.
3. **Docs internes** `docs/` (architecture, pipeline, auth, oauth, credentials,
   guardrails, deployment, env).
4. **Scrollbars + sélection de texte** thème-aware (sombre/clair).
5. **OneTimeSecret** — partage de secret sortant (team → client) dans `/admin`.
6. **Fix Stripe** — `STRIPE_SECRET_KEY` vide traité comme absent (503 honnête au lieu
   de 502). NB : c'était le bug en LOCAL ; **pas** la cause du bug prod (voir plus bas).
7. **Pricing borné** — setup bas (50/150/300€), plafond 300€ → au-delà « sur devis ».
   Fini le 1850€ pour du simple.
8. **OG / carte sociale** — `og.png` 1200×630 + meta OG/Twitter complets.

## 🐛 BUGS OUVERTS — à reprendre EN PRIORITÉ

### A. Paiement KO en PROD avec clé LIVE (cause inconnue)
- Symptôme : clic « Démarrer le projet » → « Paiement momentanément indisponible ».
- **Confirmé par owner : vu en PROD (go.pointe.dev), où la clé Stripe est la LIVE et
  valide.** Donc la cause n'est PAS la clé vide (ça c'était le local).
- À faire pour diagnostiquer : lire les logs prod au boot (`Stripe configured` ?) + au
  clic (`[stripe] checkout failed: …` donne le message d'erreur exact de l'API Stripe).
- Causes plausibles à éliminer : compte Stripe live pas pleinement activé / charges non
  autorisées ; `unit_amount` invalide ; version d'API figée `2024-12-18.acacia`.
- Owner a dit « plus tard ». Code : `crates/backend/src/handlers/stripe.rs` +
  `crates/backend/src/stripe.rs` (`create_checkout`).

### B. Pitch « sur devis » → checkout à 0 — ✅ VÉRIFIÉ OK (pas de bug)
- Risque envisagé : le pricing borné publie `price_quote = 0` quand le setup dépasse le
  cap, et Stripe rejette un `unit_amount` à 0 en mode `payment`.
- **Vérifié** : un pitch capé passe par `publish_manual_pitch` → `manual_quote: true`.
  Le front (`chat.rs:909` `if pitch_manual_quote.get()`) rend alors le bandeau « Devis
  personnalisé sous 24h » et **jamais** le bouton Stripe (qui est dans la branche `else`,
  non-manual). La chaîne `pricing_capped → manual_quote → bandeau` est cohérente.
- Rien à corriger. Gardé ici pour mémoire si le rendu CTA change un jour.

## Actions OWNER en attente (poser des secrets en prod)
- **OAuth** : `N8N_OWNER_EMAIL` / `N8N_OWNER_PASSWORD` (2FA/SSO OFF sur l'owner n8n) +
  `<PROVIDER>_OAUTH_CLIENT_ID/SECRET` par provider activé (commencer par `GOOGLE_*` qui
  couvre Gmail/Drive/Calendar/Sheets/YouTube, puis `HUBSPOT_*`). Détail → [oauth.md](oauth.md).
- **Vérification d'app OAuth** chez Google/Microsoft à anticiper avant de promettre
  Gmail/Outlook à de vrais clients (process provider de quelques jours).
- **OneTimeSecret** (optionnel) : marche en anonyme sans config ; `ONETIMESECRET_*` pour
  l'auth. Voir [env.md](env.md).

## Next steps (ordre recommandé)
1. Vérifier que le déploiement prod `b2a641d` est passé.
2. **Diagnostiquer le bug A** (paiement prod) — bloqueur d'encaissement, cœur de la
   roadmap #1.
3. **Vérifier/corriger le bug B** (checkout sur pitch capé).
4. Poser les env OAuth (action owner) → tester le bouton « Autoriser » E2E.
5. **Tiers d'exécutions** (le récurrent par exécutions/mois, soft enforcement) — suite
   logique du pricing borné, à concevoir ensemble. `price_monthly` existe déjà.

## État env LOCAL (cette machine, périmé après changement)
- `.env` local : `STRIPE_SECRET_KEY=` VIDE (≠ prod qui a la live). Si tu reprends en
  local, mettre une clé `sk_test_…` + `whsec_…` pour tester le funnel sans argent réel.
- `sharp` installé en `node_modules` (`--no-save`) pour rasteriser l'OG ; non versionné.

## Réf
- Branche source : `feat/oauth2-n8n-consent` (mergée, peut être supprimée).
- 11 commits de la session, voir `git log --oneline dd30976..b2a641d`.

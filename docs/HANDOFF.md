# Handoff

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

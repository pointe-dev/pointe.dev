# Guardrails anti-abus (ASP / clingo)

Code : `backend/src/guardrails/` (`mod.rs`, `facts.rs`, `clingo.rs`, `policy.lp`).

## Le rôle

Un contrôle **déterministe et explicable**, exécuté sur un workflow construit **avant son
activation**. Le JSON du workflow est traduit en faits ASP (`facts.rs`) puis évalué
contre une politique fixe (`policy.lp`) par clingo (`clingo.rs`). Une violation **bloque
l'auto-activation** et route le pipeline vers une revue humaine — on ne déploie jamais en
silence un workflow de spam/flood/scraping.

## Périmètre : abus SORTANT, pas autorisation d'ACCÈS

Bien distinguer deux choses **orthogonales** :

- **Cette couche (ASP) régule le SORTANT** : ce que le workflow *fait au monde* —
  envois/POST/GET répétés vers une cible. Le risque couvert est « pointe.dev devient la
  source d'un flood/scraping vers un tiers ». Pour ça, la preuve de propriété de la cible
  par **email vérifié (double opt-in / OTP) suffit** : flooder *son propre* domaine n'est
  pas une attaque tierce, donc une preuve faible est proportionnée.
- **L'accès ENTRANT** (lire le contenu d'une boîte Gmail, un Drive, etc.) n'est **PAS**
  géré ici et l'email vérifié n'y suffirait pas. Il passe par **OAuth2** (consentement du
  vrai propriétaire validé par Google/Microsoft, token scopé, aucun mot de passe stocké) —
  voir [oauth.md](oauth.md). C'est le bon mécanisme, bien plus solide qu'une vérif maison.

En résumé : ASP = *ce que le workflow a le droit de faire dehors* ; OAuth = *ce à quoi il
a le droit d'accéder*. Les deux se complètent, ne se remplacent pas.

## Pourquoi ASP et pas un LLM juge

Le signal d'abus vit dans la **structure** du workflow (fréquence de trigger, boucles,
cibles sortantes), pas dans de la prose. ASP donne un verdict complet, reproductible,
auditable — et ajouter une politique, c'est **une règle**, pas un re-prompt fragile. Le
LLM garde sa place **en amont** (intention au stade qualify) ; cette couche est le gate
structurel dur.

## Classes d'abus

**V1 — structurelles (sans notion de propriété) :**

| `kind` | Ce que ça détecte |
|--------|-------------------|
| `flood` | trigger haute fréquence alimentant une action sortante (spam/flood) |
| `mass_post` | boucle alimentant un HTTP POST externe (soumission de masse) |
| `scrape_loop` | boucle alimentant des HTTP GET externes répétés (scraping) |

**V2 — sensibles à la propriété (ownership) :**

| `kind` | Ce que ça détecte |
|--------|-------------------|
| `unowned_bulk_post` | boucle → HTTP POST vers un domaine que le client **ne possède pas** (soumission de masse contre un tiers) |
| `unowned_flood` | trigger haute fréquence frappant un domaine que le client **ne possède pas** (flood d'un service tiers) |

Chaque `Violation` sait s'expliquer en une ligne (`Violation::explain`) pour la file de
revue / la notif owner.

### Notion de propriété (ownership)

Marteler **son propre** CRM est légitime ; marteler **un tiers** ne l'est pas. On distingue
les deux via les domaines que le client a *prouvé* contrôler.

**Seule preuve acceptée aujourd'hui : le domaine de l'email vérifié (double opt-in).**
Contrôler une boîte mail sur un domaine d'entreprise est un signal fort de propriété
(seul l'admin du domaine distribue les adresses). Les webmails grand public
(gmail.com, outlook.com, proton.me…) sont **exclus** : une adresse gmail ne rend pas
gmail.com « possédé ».

> ⚠️ Le `client_n8n_url` (champ tapé librement) **n'est PAS** une preuve : l'accepter
> laisserait n'importe qui « posséder » `victim.com` en le tapant. Il est donc ignoré.
> La rigueur supérieure (vérif **DNS-TXT** / fichier `.well-known`, gold standard ACME /
> Search Console) est le prochain durcissement : elle alimentera des faits `owns_domain`
> *prouvés* via le paramètre `extra_hosts` de `owned_domains` (aujourd'hui `&[]`).

Ces domaines deviennent des faits `owns_domain(D)`. Un `http_out` vers un domaine **connu**
et **non possédé** est un `third_party` ; une URL dynamique (`{{…}}` → host `unknown`)
n'est **jamais** traitée comme non-possédée (on ne bloque pas sur un domaine illisible).

API : `evaluate_with_context(workflows, &GuardrailContext { owned_domains })`.
`evaluate(workflows)` reste l'entrée sans contexte (équivaut à `owned_domains` vide).
Les domaines possédés se dérivent via `facts::owned_domains(email, &extra_hosts)`.

## Verdict et politique fail-open / fail-closed

`evaluate(workflows) -> Verdict` :

| `Verdict` | Sens | Auto-deploy ? |
|-----------|------|---------------|
| `Allowed` | aucune violation | oui |
| `NeedsReview(violations)` | au moins une violation | **non** → revue humaine |
| `Skipped(why)` | clingo indisponible / en erreur | dépend de `fail_closed` |

`Verdict::allows_auto_deploy(fail_closed)` : `fail_closed` n'affecte **que** le cas
`Skipped`. Par défaut **fail-open** (`Skipped` laisse passer) ; mettre
`GUARDRAILS_FAIL_CLOSED=true` pour **bloquer** quand le moteur ne peut pas tourner.

`evaluate` est pur vis-à-vis des workflows (seul effet de bord : lancer clingo) et sûr sur
n'importe quelle entrée — un JSON non parsable produit simplement moins de faits.

## Où ça s'insère

Dans le pipeline, juste après `Building`/`Validating` et **avant** `Deploying` (voir
[pipeline.md](pipeline.md)). Une violation envoie vers `SavedForHuman`.

## Pré-requis ops

- Le binaire **clingo** doit être disponible (`CLINGO_PATH` ou dans le PATH).
- La politique `policy.lp` est **embarquée** (`include_str!`) — versionnée avec le code,
  une règle par classe d'abus. Pour ajouter une classe : une règle dans `policy.lp` + son
  libellé dans `Violation::explain`.

## Surfaçage admin

Les violations remontent automatiquement : le pipeline route vers
`SavedForHuman { reason }` où `reason = verdict.reason()` (liste lisible des
`explain()`), et l'admin affiche ce `stage_reason` sur chaque dossier. Pas de câblage
supplémentaire pour exposer une nouvelle classe.

## Tests

- Unitaires (sans clingo) : extraction de faits, dérivation des domaines possédés
  (`owned_domains`, exclusion webmail, dédup), assemblage du programme clingo
  (`build_fact_block` inclut bien `owns_domain`).
- End-to-end (binaire clingo requis, `#[ignore]`) : un même workflow bloqué
  (`unowned_bulk_post`) sans contexte vs **autorisé** quand le domaine est possédé ;
  `unowned_flood` ; URL dynamique non traitée comme non-possédée.
  Lancer : `cargo test -p backend -- --ignored guardrails` (clingo sur le PATH).

## Roadmap (reste V2+)

Intention LLM au stade **qualify** (couche amont complémentaire), AUP/ToS signée au
paiement, surfaçage côté **espace client**, élargissement des classes d'abus. Suivi en
mémoire projet (« Security & Guardrails Watch »).

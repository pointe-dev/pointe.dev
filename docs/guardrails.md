# Guardrails anti-abus (ASP / clingo)

Code : `backend/src/guardrails/` (`mod.rs`, `facts.rs`, `clingo.rs`, `policy.lp`).

## Le rôle

Un contrôle **déterministe et explicable**, exécuté sur un workflow construit **avant son
activation**. Le JSON du workflow est traduit en faits ASP (`facts.rs`) puis évalué
contre une politique fixe (`policy.lp`) par clingo (`clingo.rs`). Une violation **bloque
l'auto-activation** et route le pipeline vers une revue humaine — on ne déploie jamais en
silence un workflow de spam/flood/scraping.

## Pourquoi ASP et pas un LLM juge

Le signal d'abus vit dans la **structure** du workflow (fréquence de trigger, boucles,
cibles sortantes), pas dans de la prose. ASP donne un verdict complet, reproductible,
auditable — et ajouter une politique, c'est **une règle**, pas un re-prompt fragile. Le
LLM garde sa place **en amont** (intention au stade qualify) ; cette couche est le gate
structurel dur.

## Classes d'abus (V1)

| `kind` | Ce que ça détecte |
|--------|-------------------|
| `flood` | trigger haute fréquence alimentant une action sortante (spam/flood) |
| `mass_post` | boucle alimentant un HTTP POST externe (soumission de masse) |
| `scrape_loop` | boucle alimentant des HTTP GET externes répétés (scraping) |

Chaque `Violation` sait s'expliquer en une ligne (`Violation::explain`) pour la file de
revue / la notif owner.

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

## Roadmap (V2)

Règles de propriété (ownership), intention LLM au stade qualify, et surfaçage des verdicts
dans l'admin. Suivi en mémoire projet (« Security & Guardrails Watch »).

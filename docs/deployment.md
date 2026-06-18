# Déploiement

Cible prod : un serveur **Hetzner** qui tourne les images Docker via docker-compose.
Artefacts : `Dockerfile`, `docker-compose.{prod,staging}.yml`, `.github/workflows/`.

## Image Docker — le pin glibc (ne pas le casser)

Le `Dockerfile` est un build multi-stage :

- **builder** : `rust:1-bookworm`
- **runtime** : `debian:bookworm-slim`

Les deux **doivent rester sur bookworm**. `rust:latest` a dérivé vers une Debian plus
récente (glibc 2.39) et a produit un binaire que le runtime (glibc 2.36) ne pouvait pas
charger. Le builder et le runtime partagent la même version de glibc grâce à ce pin —
le commentaire en tête du `Dockerfile` l'explique. Si un jour tu bumpes l'un, bumpe
l'autre en même temps.

## CI / CD — `.github/workflows/`

| Workflow | Rôle |
|----------|------|
| `ci.yml` | tests / lint sur PR |
| `deploy.yml` | déploiement prod |
| `deploy-staging.yml` | déploiement staging (voir [STAGING.md](STAGING.md)) |

**Stratégie d'images** : le build pousse vers GHCR taggé `latest` **et** `sha-<sha>`. Le
workflow de deploy ne fait **que pull + restart** (pas de rebuild sur le serveur) : il se
connecte en SSH (`appleboy/ssh-action`), `docker login ghcr.io`, pull, redémarre.

### Rollback

Comme chaque build est taggé `sha-<sha>`, revenir en arrière = pull du sha précédent :

```bash
docker pull ghcr.io/<owner>/<image>:sha-<fullsha>
# puis repointer le compose sur ce tag et redémarrer
```

(Voir la mémoire « Deploy: glibc pin & rollback » pour le détail.)

## Secrets en prod

Les secrets à fallback fichier (`SESSION_SECRET`, `ADMIN_INGEST_TOKEN`) peuvent être
montés en fichier via `*_FILE` plutôt qu'en variable d'env. **`SESSION_SECRET` doit être
persistant** : s'il est régénéré à chaque boot, toutes les sessions sont invalidées.
Liste complète : [env.md](env.md).

## `infra/terraform/` — lab d'apprentissage (PAS la prod)

Le dossier `infra/terraform/` est un **lab Terraform jetable** sur Hetzner (apprentissage
du cycle `init → plan → apply → destroy`), **pas** l'infra de production. Voir
`infra/terraform/README.md`. Token Hetzner passé via `TF_VAR_hcloud_token` (jamais dans un
fichier). Au moment d'écrire, ce lab est en pause à l'étape `apply` — rien n'a été créé ni
facturé (mémoire « Infra Lab 01 PAUSED »).

> Note : `infra/` n'est pas (encore) commité dans le repo principal.

## Lancer en local

Voir `README.md` à la racine. En résumé :

```bash
npm install                 # Tailwind
npm run tailwind:build      # input.css → styles.css
cargo run -p backend        # le serveur
cargo leptos watch          # full stack en dev
```

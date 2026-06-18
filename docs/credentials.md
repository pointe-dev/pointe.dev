# Credentials & catalogue de capacités

Comment le client connecte ses comptes au workflow livré. Deux chemins selon le mode
d'auth du service : **clé API** (ce doc) ou **OAuth2** ([oauth.md](oauth.md)).

Code : `backend/src/capabilities.rs` (catalogue), `backend/src/credentials.rs` (client
REST n8n), handler `provision` dans `handlers/credentials.rs`, UI dans
`frontend/src/components/delivery.rs`.

## Le catalogue — `capabilities.rs`

`CATALOG: &[Capability]` est la liste **curée** des services qu'on sait livrer. Chaque
entrée :

```rust
Capability {
    service: "Notion",                 // nom canonique affiché
    aliases: &["notion"],              // pour matcher un nom depuis le besoin/recherche
    tier: Tier::Native,                // native | http | managed
    auth: Auth::ApiKey,                // None | ApiKey | OAuth2
    cred_type: Some("notionApi"),      // type de credential n8n, ou None si pas câblé
}
```

Règle d'or (commentaire en tête du catalogue) : **sous-lister ne fait que sous-promettre
(safe), jamais sur-promettre.** N'ajoute un service qu'une fois sa couverture confirmée.

`classify(name)` fait un match insensible à la casse du `name` contre les `aliases` et
renvoie la capacité.

### Champ `cred_type` selon le mode d'auth

- **`Auth::ApiKey`** : `cred_type` = type de credential n8n pour le provisioning en
  self-service (ex : `notionApi`, `stripeApi`). `None` = provisioning pas câblé → la
  delivery le marque comme manuel.
- **`Auth::OAuth2`** : `cred_type` = type OAuth n8n (ex : `gmailOAuth2`). Nécessaire au
  flux de consentement (voir [oauth.md](oauth.md)). Tous les services OAuth2 du catalogue
  sont câblés.
- **`Auth::None`** : webhook, cron, RSS — aucun identifiant requis.

`Auth::prerequisite()` donne le libellé human de ce qu'il faut fournir (« clé API à
fournir » / « connexion OAuth à autoriser »).

### `provisionable` (vu côté delivery)

Calculé dans `handlers/pipeline.rs` : `auth == ApiKey && cred_type.is_some()`. Donc
**les services OAuth2 ne sont jamais `provisionable`** (pas de saisie de clé) — ils
passent par le bouton « Autoriser » de l'OAuth.

## Provisioning par clé API — `POST /api/credentials/provision`

Flux (`credentials.rs` + handler `provision`) :

1. Le front envoie `{ session_id, service, secrets: { apiKey, accessToken, token } }`.
   Il envoie la valeur sous **plusieurs noms de champ courants** ; le backend ne garde
   que celui présent dans le schéma du credential (et jette le reste).
2. `N8nRestConfig::credential_schema` récupère le schéma du `cred_type` depuis n8n.
3. `build_credential_data` ne retient que les champs du schéma.
4. `create_credential` crée le credential dans n8n via `/api/v1` (clé `N8N_API_KEY`).

`N8nRestConfig::from_env` lit `N8N_URL` + `N8N_API_KEY`. À distinguer de `N8nOwnerLogin`
(login `/rest` par email/mot de passe) utilisé par l'OAuth — voir [oauth.md](oauth.md)
pour pourquoi les deux coexistent (l'API publique vs l'API UI session-gated).

## UI de livraison — `delivery.rs`

Le composant `delivery_row` rend une ligne par intégration :

| Cas | Rendu |
|-----|-------|
| `provisionable` (API key) | champ + bouton « Connecter » → `/api/credentials/provision` |
| `auth == oauth2` | bouton « Autoriser la connexion » → `/api/oauth/start` (voir oauth.md) |
| `tier == managed` | « Intégration sur mesure — réalisée par notre équipe » |
| sinon | « Aucun identifiant requis » |

Réutilisé par la page post-paiement (`/merci`) et l'espace client (`/espace`).

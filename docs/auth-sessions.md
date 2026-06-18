# Auth & sessions

Code : `backend/src/sessions.rs`, handlers `/api/auth/*` dans `handlers/` + le gate dans
le chat (`/api/ai/chat`).

## Le modèle : free-tier puis double opt-in

```
Visiteur anonyme
   │  jusqu'à FREE_MESSAGES (= 5) messages gratuits dans le chat
   ▼
6e message bloqué → on demande l'email
   │  email saisi → on envoie un lien de confirmation (Resend)
   ▼
Clic sur le lien (double opt-in) → session "unlocked" → chat illimité
```

`FREE_MESSAGES = 5` (constante en haut de `sessions.rs`).

## Le `SessionStore`

Store à deux niveaux (`sessions.rs`) :

- **L1 — mémoire** (`HashMap` sous `RwLock`) : le chemin chaud reste lock-only, aucun
  `await` DB sous charge.
- **L2 — Postgres** (optionnel, write-through) : survit aux redémarrages. Au boot,
  `with_db` **hydrate** les maps depuis les tables `sessions` / `fp_limits`. Sans
  `DATABASE_URL`, le store est purement en mémoire (`new()`).

Deux maps :

| Map | Clé | Rôle |
|-----|-----|------|
| `sessions` | `session_id` (UUID ou token signé) | compteur de messages, `unlocked`, email |
| `fp_limits` | `SHA-256(ip \| fingerprint)` | rate-limit secondaire |

Le `fp_limits` existe pour qu'**effacer le localStorage ne remette pas le free-tier à
zéro** : même si le client jette son `session_id`, son empreinte IP+fingerprint reste
plafonnée (`fp_bucket(ip, fingerprint)`).

## API publique du store (sélection)

| Fonction | Rôle |
|----------|------|
| `check_and_increment` | incrémente le compteur, applique le gate free-tier + le rate-limit empreinte |
| `unlock_with_email` | passe la session en `unlocked` après confirmation |
| `is_unlocked` | le gate lu par les endpoints protégés (ex : `/api/oauth/start`) |
| `message_count` / `get_email` | lectures |

## Tokens signés (HMAC-SHA256)

Le secret est `SESSION_SECRET` (voir [env.md](env.md) — fallback fichier puis généré).

- `sign_token` / `verify_token` : token de session porté par le client.
- `sign_confirm_token` / `verify_confirm_token` : token du **lien de confirmation** email
  (lie `email` + `session_id`, à usage unique logique).
- `looks_like_token` : distingue un `session_id` qui est un token signé d'un simple UUID.

## Endpoints

| Route | Rôle |
|-------|------|
| `POST /api/auth/unlock` | soumet l'email → déclenche l'envoi du lien de confirmation |
| `GET /api/auth/confirm` | cible du lien email → unlock la session |
| `GET /api/auth/status` | état courant (compteur, unlocked) pour le front |

Côté front, le `session_id` est gardé dans `localStorage` sous `_sid`
(`delivery.rs::session_id`) et renvoyé sur les appels qui exigent une session débloquée.

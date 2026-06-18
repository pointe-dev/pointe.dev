# Connexion OAuth2 des comptes clients

Code : `backend/src/oauth.rs`, handler `oauth_start` dans
`backend/src/handlers/credentials.rs`, bouton front dans
`frontend/src/components/delivery.rs`.

## Les deux niveaux de secrets (à ne jamais confondre)

| Niveau | C'est quoi | À qui | Où il vit |
|--------|-----------|-------|-----------|
| **App OAuth** (`client_id` / `client_secret`) | L'identité de *l'application pointe.dev* enregistrée chez le provider (Google Cloud Console, etc.). **Une seule app par provider, partagée par tous les clients.** | **Nous** (owner) | env serveur : `<PROVIDER>_OAUTH_CLIENT_ID/SECRET` |
| **Token client** (access/refresh) | Le droit d'accès au compte d'**un** client précis, émis quand il clique « Autoriser ». | Le client | **n8n** le reçoit, le chiffre, le stocke. On ne le voit jamais. |

> Analogie : l'app OAuth = la serrure de la porte d'agence (installée une fois). Le token
> client = la clé personnelle remise à chaque client pour cette serrure.

Le client **ne saisit jamais** de `client_id`/`client_secret` — ce sont nos secrets. Il
clique « Autoriser » et accepte chez le provider, comme un « Se connecter avec Google ».

## Pourquoi n8n fait tout le sale boulot

n8n possède le dance OAuth2 complet : échange code→token, refresh, stockage chiffré.
Notre rôle se limite à : (1) créer la *coquille* de credential (clientId/clientSecret/scopes)
via l'API REST publique, puis (2) donner au client l'URL de consentement du provider.

**Le hic qui façonne le module** : l'endpoint qui *fabrique* cette URL,
`GET /rest/oauth2-credential/auth?id=<credId>`, vit sous l'API UI `/rest/` de n8n,
gardée par un **cookie de session de login** — PAS par la clé d'API publique `/api/v1`.
Donc pour piloter le consentement par programme, on se logge une fois avec le compte
**owner** n8n, on réutilise le cookie de session pour récupérer l'URL, et on redirige le
client. Le callback de n8n (`/rest/oauth2-credential/callback`) finit l'échange — on
n'écrit aucun handler de callback et on ne détient aucun token.

## Quand ça se déclenche

**Après paiement, pendant la mise en place du workflow du client.** Sur la checklist de
livraison (page `/merci` et l'espace `/espace`, composant `delivery.rs`), chaque service
OAuth2 affiche un bouton « Autoriser la connexion ».

## Le flux, étape par étape

```
[Une fois, owner]  Enregistrer l'app OAuth pointe.dev chez le provider
                   → poser GOOGLE_OAUTH_CLIENT_ID/SECRET (etc.) dans l'env serveur
───────────────────────────────────────────────────────────────────────────
[Par client, après paiement]

1. Client clique « Autoriser la connexion » sur sa checklist
2. Navigateur → POST /api/oauth/start { session_id, service }   (aucune clé envoyée)
3. Backend (oauth_start) :
     a. gate : la session doit être unlocked (sinon 403)
     b. classify(service) → doit être OAuth2 avec un cred_type câblé
     c. oauth::app_keys_for(cred_type) → lit NOS clés d'app depuis l'env
     d. credentials::create_credential → crée la coquille dans n8n (nos clés)
     e. N8nOwnerLogin::consent_url → login owner /rest, récupère l'URL de consentement
     → renvoie { status: "consent", credential_id, consent_url }
4. Navigateur redirige (window.location) vers consent_url
5. Le client accepte chez le provider (c'est LUI qui s'authentifie ici)
6. Provider → callback n8n → n8n échange le code, chiffre et stocke le token DU CLIENT
7. Le workflow du client peut utiliser la connexion.
```

## Réponses de `/api/oauth/start`

`OauthStartResponse` (tag `status`, snake_case) :

- `consent` → `{ service, credential_id, consent_url }` : rediriger le navigateur.
- `not_oauth` → le service n'est pas OAuth2 (ou pas catalogué) ; utiliser `/provision`.
- `not_wired` → service OAuth2 mais **soit** son `cred_type` n8n n'est pas câblé,
  **soit** nos clés d'app ne sont pas configurées dans l'env → handoff manuel.

## Mapping env des clés d'app

`oauth::app_keys_for(cred_type)` lit `<PREFIX>_OAUTH_CLIENT_ID` / `_CLIENT_SECRET`.
Plusieurs providers Google partagent **une seule** app Google (préfixe `GOOGLE`) :

| cred_type n8n | Préfixe env |
|---------------|-------------|
| `gmailOAuth2`, `googleDriveOAuth2Api`, `googleCalendarOAuth2Api`, `googleSheetsOAuth2Api`, `youTubeOAuth2Api` | `GOOGLE` |
| `microsoftOutlookOAuth2Api` | `MICROSOFT` |
| `slackOAuth2Api` | `SLACK` |
| `hubspotOAuth2Api` | `HUBSPOT` |
| `salesforceOAuth2Api` | `SALESFORCE` |
| `zohoOAuth2Api` | `ZOHO` |
| `mailchimpOAuth2Api` | `MAILCHIMP` |
| `asanaOAuth2Api` | `ASANA` |
| `shopifyOAuth2Api` | `SHOPIFY` |
| `twitterOAuth2Api` | `TWITTER` |
| `linkedInOAuth2Api` | `LINKEDIN` |

La source de vérité est le `match` dans `oauth::app_keys_for` — tiens cette table
synchronisée avec lui.

## Pré-requis ops / pièges

- Poser `N8N_OWNER_EMAIL` / `N8N_OWNER_PASSWORD` (login owner pour minter l'URL) et la
  paire `<PREFIX>_OAUTH_CLIENT_ID/SECRET` de chaque provider activé.
- **2FA/SSO doit être désactivé** sur le compte owner n8n : le login `/rest` se fait par
  email+mot de passe. Avec 2FA/SSO, `consent_url` échoue (« login succeeded but set no
  session cookie »). Documenté en tête de `oauth.rs`.
- **Vérification de l'app OAuth chez le provider** : une app en mode test/non-vérifiée
  limite le nombre d'utilisateurs et affiche un avertissement (« app non vérifiée »).
  Pour de vrais clients, faire vérifier l'app (Google/Microsoft : process de quelques
  jours). Purement côté provider, à anticiper avant de promettre Gmail/Outlook en prod.

## Pourquoi un client HTTP dédié (et pas `state.http`)

`consent_url` crée un client `reqwest` avec un jar de cookies par appel : le login dépose
le cookie de session dans ce jar, qui sert ensuite pour la requête d'URL. Le client
global `state.http` reste sans cookies. Pas de cache de session entre appels (les logins
sont rares — un par connexion de compte), ce qui garde le module stateless et évite de
garder le cookie owner en mémoire.

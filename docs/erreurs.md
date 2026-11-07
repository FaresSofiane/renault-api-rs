# Erreurs et dépannage

[Sommaire](README.md) · [Authentification](authentification.md)

## Traiter une erreur

Toutes les opérations fallibles retournent `Result<T, RenaultError>`.
L’enum est `#[non_exhaustive]` : garder une branche de repli dans les matchs.

```rust,no_run
# async fn read(api: &renault_api::RenaultApi) -> renault_api::Result<()> {
use renault_api::{ApiErrorKind, RenaultError};

match api.cockpit("account-id", "VF1AAAAAAAAAAAAAA").await {
    Ok(cockpit) => println!("Kilométrage : {:?}", cockpit.total_mileage_km),
    Err(RenaultError::AuthenticationExpired | RenaultError::NotAuthenticated) => {
        // Proposer de rétablir la session dans l’application.
    }
    Err(RenaultError::Api { kind: ApiErrorKind::QuotaExceeded, .. }) => {
        // Espacer les lectures ; aucun retry immédiat n’est imposé par la crate.
    }
    Err(error) => return Err(error),
}
# Ok(())
# }
```

## Erreurs locales et d’authentification

| Variante | Cause | Réaction |
|---|---|---|
| `Configuration(String)` | Locale/pays/URL/clé/timeout invalide, ou initialisation TLS impossible | Corriger la configuration |
| `InvalidInput(String)` | Valeur vide, identifiant invalide, cookie interdit, adresse A2F inconnue, JWT sans personId pour découvrir les comptes | Corriger l’entrée ou choisir un mode d’authentification adapté |
| `NotAuthenticated` | Aucun jeton utilisable n’est installé | Appeler `authenticate` |
| `AuthenticationExpired` | JWT expiré sans renouvellement possible, session révoquée, ou rejet reconnu du JWT | Réessayer une lecture après renouvellement possible ou reconnecter selon le contexte |
| `InvalidCredentials` | Code Gigya `403042` | Redemander les identifiants |
| `InvalidAccessToken` | JWT sans trois parties non vides, payload invalide ou claim exp absent/invalide | Fournir un JWT valide |

`Authenticated` après installation locale d’un JWT ne prouve pas que le serveur
l’acceptera. Un JWT rejeté peut produire une erreur lors du premier appel réseau.

## Erreurs A2F

| Variante | Cause | Réaction |
|---|---|---|
| `NoPendingTwoFactor` | Aucun challenge actif | Refaire un login si nécessaire |
| `VerificationCodeNotRequested` | Validation demandée avant l’envoi du code | Appeler `request_two_factor_code` |
| `InvalidVerificationCode` | Code Gigya `400006` sur l’endpoint de vérification | Garder le client et redemander le code saisi |
| `EmailSelectionRequired` | Plusieurs adresses et aucun ID choisi | Faire choisir une adresse |
| `NoTwoFactorEmail` | Aucune adresse éligible | Vérifier le compte et son parcours d’authentification |

Un code Gigya `400006` reçu ailleurs que sur la validation du code reste une
erreur API générique. Le contexte de l’endpoint est donc conservé dans la logique.

## Transport et réponses

| Variante | Contenu | Signification |
|---|---|---|
| `Transport(reqwest::Error)` | Source Reqwest sans URL | DNS, timeout, TLS, socket ou lecture réseau échouée |
| `Http { service, status }` | Service et statut HTTP | Échec HTTP sans erreur métier reconnue prioritaire |
| `Api { service, code, kind, status }` | Code brut, classification, statut | Erreur Gigya/Kamereon décodée |
| `InvalidResponse { service, context }` | Contexte statique | JSON, enveloppe, type, date ou valeur invalide ; corps supérieur à la limite |
| `UnsupportedEndpoint { endpoint, model_code }` | Fonction et modèle | Fonction connue comme indisponible, refusée avant sa requête de télémétrie |

Les champs `service` utilisent `Service::Gigya` ou `Service::Kamereon`.
Une erreur métier peut être présente dans une réponse HTTP 200. La bibliothèque
la vérifie au lieu de considérer le statut seul comme un succès.

Les réponses ne sont pas toutes interprétables : une structure malformée peut
produire `InvalidResponse` même si un statut HTTP d’erreur est également présent.
Le corps et les messages serveur ne sont pas inclus dans les erreurs publiques.

## Classification Kamereon

| Code reçu | `ApiErrorKind` / erreur |
|---|---|
| `err.func.400` | `InvalidInput` |
| `err.func.403`, `err.func.wired.forbidden` | `AccessDenied` |
| `err.tech.501` | `NotSupported` |
| `err.func.wired.notFound`, `err.func.wired.not-found` | `NotFound` |
| `err.func.wired.overloaded` | `QuotaExceeded` |
| `err.func.privacy.on` | `PrivacyMode` |
| `err.tech.500`, `err.tech.wired.kamereon-proxy` | `Upstream` |
| `err.func.wired.unauthorized` | `RenaultError::AuthenticationExpired` |
| Autre code | `Other`, avec code brut conservé |

Un HTTP 429 sans ce code métier peut rester `Http { status: 429, .. }`.
Prévoir les deux formes pour sa politique de quota. La V1 n’expose pas de champ
`Retry-After` et ne déduit pas de fréquence d’interrogation autorisée.

## Renouvellement et retry

À expiration locale, le client possédant un jeton de session demande un nouveau
JWT avant l’appel protégé. Si le serveur rejette un JWT pourtant encore frais,
l’appel retourne une erreur et le JWT concerné est invalidé localement. Un appel
suivant peut le renouveler. Le premier appel n’est pas rejoué automatiquement.

Une erreur d’expiration reconnue sur la demande Gigya de JWT ou d’informations
de compte efface la session locale : réutiliser le même jeton révoqué en boucle
ne résout pas le problème.

L’application peut définir des retries espacés pour les lectures. Éviter de
répéter automatiquement l’envoi d’un code A2F : cela envoie réellement un email
et remplace le jeton de vérification précédent.

## Dépannage par symptôme

| Symptôme | Vérification utile |
|---|---|
| Panic indiquant l’absence de reactor Tokio | Exécuter le client async dans un runtime Tokio |
| Interface figée | Vérifier les tâches synchrones et l’intégration du runtime |
| JWT systématiquement expiré | Vérifier l’horloge, `exp` et la marge de 60 secondes |
| Comptes introuvables avec un JWT | Vérifier personId ; fournir plutôt un jeton de session |
| Fonction refusée sur un seul véhicule | Vérifier modèle, contrat et services disponibles |
| Position absente | Vérifier support du modèle, mode privé et date du relevé |
| Puissance surprenante | Vérifier modèle et unité ; les mesures Renault peuvent être erronées |
| Timeout global supérieur au réglage | Une méthode compose plusieurs requêtes et peut attendre un verrou |
| TLS en entreprise | Vérifier l’autorité du proxy ; la crate utilise des racines embarquées |

## Rapports de problème

Fournir la version de la crate, le système et son architecture, le nom de la
méthode, le code modèle, la variante d’erreur et les codes/statuts non sensibles.
Ne pas joindre mot de passe, jeton, URL A2F complète, VIN réel ou corps brut
contenant des données personnelles.

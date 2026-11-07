# Renault API — Rust

Client Rust asynchrone indépendant pour l’API privée utilisée par MyRenault.
Cette crate n’a aucun lien avec `car_app` : le dossier peut être déplacé dans
son propre dépôt sans modifier le code ou les tests.

La [documentation complète](docs/README.md) détaille l’installation, l’A2F,
chaque méthode et chaque champ, les erreurs, la concurrence et les plateformes.

## Périmètre V1

- Connexion par email/mot de passe, jeton de session Gigya ou JWT d’accès.
- A2F par code email : choix de l’adresse, envoi, validation, nouvelle tentative
  après un code incorrect et reconnaissance de l’appareil.
- Comptes, véhicules, détails, batterie/autonomie, cockpit/kilométrage et position.
- Réponses typées, unités explicites, dates UTC et champs absents en `Option`.
- Renouvellement des JWT en mémoire et erreurs classifiées.

Aucun stockage sur disque, base de données, CLI, interface graphique, polling,
intégration Home Assistant ou binding UniFFI. Les commandes de charge/climatisation,
les historiques, les contrats et les programmations ne font pas partie de la V1.

## Installation asynchrone

Dans le `Cargo.toml` de l’application consommatrice :

```toml
[dependencies]
renault-api = { path = "../renault-api-rs" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Les opérations réseau s’exécutent dans le runtime Tokio fourni par l’application.
Le client asynchrone ne crée pas de runtime ni de thread en arrière-plan.

## Depuis un jeton de session

```rust,no_run
use renault_api::{Auth, Config, RenaultApi, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let api = RenaultApi::new(Config::new("fr_FR")?)?;
    api.authenticate(Auth::SessionToken("jeton-gigya".into())).await?;

    for account in api.accounts().await? {
        for vehicle in api.vehicles(&account.id).await? {
            let cockpit = api.cockpit(&account.id, &vehicle.vin).await?;
            println!("Kilométrage : {:?} km", cockpit.total_mileage_km);
            // Selon les fonctions disponibles sur ce véhicule :
            // let battery = api.battery_status(&account.id, &vehicle.vin).await?;
            // let location = api.location(&account.id, &vehicle.vin).await?;
        }
    }
    Ok(())
}
```

`Auth::SessionToken` vérifie le jeton en demandant un JWT. Les connexions par mot
de passe obtiennent un jeton de session ; leur JWT est demandé au premier appel
qui en a besoin. Il est renouvelé avant expiration, avec une marge de 60 secondes.
Les appels simultanés partagent un seul renouvellement.

`Auth::AccessToken(jwt)` accepte un JWT non expiré. Son payload est décodé pour
lire `exp` et `personId` ; **ce décodage ne vérifie pas sa signature**. Le service
Renault reste responsable de sa validation. Sans jeton de session, aucun
renouvellement n’est possible. Sans claim `personId`, les appels avec compte/VIN
explicites restent utilisables, mais `accounts()` nécessite un jeton de session.

## Connexion et A2F

```rust,no_run
use renault_api::{Auth, AuthOutcome, Config, RenaultApi, RenaultError, Result};

async fn authenticate(email: String, password: String, code: String) -> Result<(RenaultApi, AuthOutcome)> {
    let api = RenaultApi::new(Config::new("fr_FR")?)?;
    let mut outcome = api.authenticate(Auth::Credentials { email, password }).await?;
    match outcome {
        AuthOutcome::Authenticated => {}
        AuthOutcome::TwoFactorRequired => {
            let emails = api.two_factor_emails().await?;
            // L’interface laisse choisir l’adresse si plusieurs sont disponibles.
            let selected_email = match emails.as_slice() {
                [email] => &email.id,
                [] => return Err(RenaultError::NoTwoFactorEmail),
                _ => return Err(RenaultError::EmailSelectionRequired),
            };
            api.request_two_factor_code(Some(selected_email)).await?;
            // À cet instant, l’interface demande à l’utilisateur le code reçu.
            // Ici `code` représente la valeur fournie par cette interface.
            outcome = api.complete_two_factor(&code, true).await?;
        }
    }
    Ok((api, outcome))
}
```

Dans une interface réelle, conserver le même client entre l’envoi et la saisie du
code. Si une seule adresse est disponible, `request_two_factor_code(None)` la
sélectionne ; avec plusieurs adresses, cette méthode retourne
`RenaultError::EmailSelectionRequired`.

Un code incorrect retourne `InvalidVerificationCode` et permet de rappeler
`complete_two_factor`. Une nouvelle demande de code remplace le précédent.
`cancel_two_factor()` efface le challenge et le mot de passe temporaire.
Après une validation réussie, une nouvelle tentative reprend la finalisation ou
la connexion sans soumettre à nouveau le code déjà validé. Un résultat
`TwoFactorRequired` lors de la reconnexion signifie qu’un nouveau challenge
doit être traité ; l’application doit toujours examiner le résultat.

## Reconnexion sans stockage intégré

```rust,no_run
use renault_api::{Auth, Config, RenaultApi, Result};

async fn restore_example(api: &RenaultApi) -> Result<()> {
    if let Some(session) = api.session_credentials().await {
        // L’application peut choisir de sauvegarder `session` dans son coffre.
        // La bibliothèque ne l’écrit jamais sur disque.
        let restored = RenaultApi::new(Config::new("fr_FR")?)?;
        restored.authenticate(Auth::RestoredSession(session)).await?;
    }
    Ok(())
}
```

`SessionCredentials` contient le jeton de session et les cookies d’identification
de l’appareil (`gmid`/`ucid`), sans mot de passe ou challenge A2F. Cette valeur est
sensible, même si son affichage `Debug` est masqué. Elle est sérialisable pour
laisser l’application choisir sa politique de sauvegarde. Pour un nouveau login
par mot de passe, `set_device_ids` permet de réinjecter les identifiants de
l’appareil avant `authenticate`.

Les clones de `RenaultApi` partagent la session. `logout()` efface localement la
session de tous les clones, les cookies et les métadonnées ; il ne révoque pas la
session côté Renault. Les requêtes déjà parties peuvent encore se terminer.

## Contrat des lectures et des erreurs

| Méthode | Résultat |
|---|---|
| `accounts()` | `Vec<Account>` |
| `vehicles(account_id)` | `Vec<Vehicle>` |
| `vehicle_details(account_id, vin)` | `VehicleDetails` |
| `battery_status(account_id, vin)` | `BatteryStatus` |
| `cockpit(account_id, vin)` | `Cockpit` |
| `location(account_id, vin)` | `Location` |

Toutes retournent un `Result<T, RenaultError>`. Les codes Renault inconnus sont
conservés dans `RenaultError::Api` avec le service, le statut HTTP et une
classification. Les corps d’erreur et les URL d’authentification ne sont pas
inclus dans les erreurs affichables. La bibliothèque ne journalise pas les
requêtes ni les réponses.

Les relevés ne sont pas nécessairement en temps réel : leur horodatage est
conservé lorsqu’il existe. La puissance est normalisée en kW (division par 1000
sur X101VE), le kilométrage et l’autonomie en km. Les états de branchement et de
charge inconnus conservent leur valeur brute. Les mesures erronées produites par
le véhicule ne peuvent pas être corrigées par la seule conversion d’unités.

Les listes de véhicules alimentent un petit cache de **codes modèles en mémoire**,
utilisé pour les unités et les fonctions connues comme indisponibles. Si le code
modèle manque, `battery_status` et `location` demandent d’abord les détails.
`vehicle_details` actualise explicitement ces métadonnées. Aucun relevé de
télémétrie n’est mis en cache.

La compatibilité connue par modèle provient du Python. Pour un modèle inconnu,
la route standard est essayée ; cela ne garantit pas que le contrat ou le
véhicule autorise la fonction. Les erreurs Renault restent exposées.

Les requêtes ont une durée maximale de 30 secondes (configurable), une limite
de réponse de 2 MiB et ne suivent pas les redirections. Aucun retry HTTP
automatique : si Renault rejette un JWT, l’appel retourne une erreur et l’appel
suivant pourra en obtenir un nouveau si un jeton de session est disponible.

Les locales connues de la copie Python sont incluses, avec leurs régions et clés
publiques d’application. Pour une autre locale ou une configuration Renault
modifiée, utiliser `Config::with_endpoints` et `Endpoints::new`. Les racines
requièrent HTTPS ; HTTP n’est accepté que sur loopback pour les tests locaux.

## Plateformes

Le cœur utilise Rustls avec un provider `ring` et les racines Mozilla embarquées.
Il n’exige ni OpenSSL installé, ni initialisation d’un vérificateur de certificats
dans la JVM Android. Les racines embarquées doivent être maintenues à jour via
les dépendances. Les certificats privés ajoutés au système ne sont pas utilisés.

Cette crate expose une API **Rust**, sans ABI C ou bindings Swift/Kotlin. Les
bindings et le conditionnement natif seront des couches séparées. Le workflow
`.github/workflows/ci.yml` est prévu pour un dépôt contenant cette crate à sa
racine : tests natifs macOS/Windows/Linux, vérification iOS et Android. Tant que
ce dossier reste imbriqué ici, GitHub ne découvre pas ce workflow automatiquement.

## Vérification

Depuis ce dossier :

```sh
cargo test --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo check --locked --target aarch64-apple-ios
```

Les tests utilisent uniquement des serveurs HTTP sur loopback et des fixtures
anonymisées. Ils n’utilisent pas de compte réel et ne contactent pas Renault.
Les exemples peuvent être compilés sans les exécuter avec
`cargo check --examples`.

## Origine

Protocole et fixtures adaptés de la copie locale de
[hacf-fr/renault-api](https://github.com/hacf-fr/renault-api), sous licence MIT
(copyright 2020 epenet). La notice d’origine est conservée dans `LICENSE`.
La bibliothèque utilise des endpoints privés, sans garantie de stabilité de leur
part ; elle n’est pas affiliée à Renault.

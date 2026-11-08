# Renault API — Rust

Client Rust asynchrone pour l’API Renault, avec une interface `RenaultApi`
et des résultats structurés. La crate est utilisable depuis toute application
Rust disposant d’un runtime Tokio, sur macOS, Windows, Linux, Android et iOS.

La [documentation complète](docs/README.md) décrit chaque méthode, les types,
l’A2F, les commandes et les variantes de véhicules.
La [matrice de parité](docs/parite.md) indique ce qui est repris du Python et
ce qui est volontairement exclu.

## Périmètre

- Connexion par identifiants, session Gigya ou JWT ; A2F email et restauration de session.
- Comptes, véhicules, détails, adaptateur, batterie/SoC, pneus, position, portes, climatisation, charge et notifications.
- Contrats, alertes, historiques de charge et de climatisation, calendriers KCA et réglages KCM.
- Commandes de charge/climatisation, programmations, seuils de batterie, klaxon, éclairage et demande de position.
- Routage de 28 fonctions pour les 25 codes modèles documentés, client HTTP bas niveau et découverte de configuration régionale.

Aucun stockage persistant, CLI applicative, intégration Home Assistant ou polling.
La bibliothèque gère l’authentification, les appels HTTP et les types de données.
L’application appelante choisit comment exploiter les résultats.

## Installation depuis crates.io

Après publication de la version `0.1.0` :

```toml
[dependencies]
renault-api = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Le client utilise le runtime Tokio fourni par l’application appelante.

## Première lecture

```rust,no_run
use renault_api::{Auth, Config, RenaultApi, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let api = RenaultApi::new(Config::new("fr_FR")?)?;
    api.authenticate(Auth::SessionToken("jeton-de-session".into())).await?;
    for account in api.accounts().await? {
        for vehicle in api.vehicles(&account.id).await? {
            let cockpit = api.cockpit(&account.id, &vehicle.vin).await?;
            println!("Kilométrage : {:?} km", cockpit.total_mileage_km);
        }
    }
    Ok(())
}
```

Les relevés ne sont pas nécessairement en temps réel. Les valeurs absentes
restent `None`, et leurs horodatages sont conservés. Une fonction peut être
refusée selon le modèle, le contrat ou les services réellement activés.

## Connexion et A2F

```rust,no_run
use renault_api::{Auth, AuthOutcome, Config, RenaultApi, Result};

async fn begin_login(email: String, password: String) -> Result<(RenaultApi, AuthOutcome)> {
    let api = RenaultApi::new(Config::new("fr_FR")?)?;
    let outcome = api.authenticate(Auth::Credentials { email, password }).await?;
    if outcome == AuthOutcome::TwoFactorRequired {
        let emails = api.two_factor_emails().await?;
        // L’interface fait choisir une adresse si plusieurs sont disponibles.
        if emails.len() == 1 {
            api.request_two_factor_code(None).await?;
        }
        // Conserver le client ; après la saisie du code :
        // let outcome = api.complete_two_factor(&code, true).await?;
    }
    Ok((api, outcome))
}
```

Un code incorrect peut être ressaisi sur le même challenge. Le mot de passe est
conservé temporairement seulement lorsque le second login du parcours A2F le
nécessite. Voir le [guide d’authentification](docs/authentification.md).

Un jeton de session permet d’obtenir de nouveaux JWT ; un JWT seul ne peut pas
être renouvelé. Son payload est inspecté pour `exp` et `personId`, sans
vérification locale de signature. Renault valide le jeton sur les appels réseau.

## Session exportable, sans stockage intégré

```rust,no_run
use renault_api::{Auth, Config, RenaultApi, Result};

async fn restore_example(api: &RenaultApi) -> Result<()> {
    if let Some(session) = api.session_credentials().await {
        let restored = RenaultApi::new(Config::new("fr_FR")?)?;
        restored.authenticate(Auth::RestoredSession(session)).await?;
    }
    Ok(())
}
```

L’application décide d’une éventuelle sauvegarde du jeton de session et des
identifiants d’appareil. `logout` efface l’état local partagé entre clones, sans
révocation distante ni annulation des requêtes déjà parties.

## Commandes

Les méthodes retournent un `CommandReceipt`, qui confirme la réponse HTTP et
l’accusé de réception disponible, pas l’exécution physique par le véhicule.
Aucune commande n’est rejouée automatiquement.

Sur les véhicules KCM utilisant `ev/settings`, `start_charge(..., None)` lit les
réglages, désactive **tous** les programmes planifiés, y compris ceux de
préconditionnement, puis renvoie le document modifié en conservant les champs
inconnus. Cette conséquence fait partie du comportement repris du Python.
Une date de charge différée est refusée lorsque la route ne la prend pas en
charge, au lieu d’être ignorée silencieusement.

Voir [Commandes et programmations](docs/commandes.md) et la
[référence API](docs/reference-api.md) pour les paramètres et les variantes.

## Vérifier

```sh
cargo test --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo check --examples --locked
```

Les tests utilisent uniquement des serveurs loopback et les fixtures anonymisées.
Les extraits Rust des guides sont compilés sans les exécuter. Les tests couvrent
la matrice de routes, les corps de commandes et les variantes KCA/KCM ; ils ne
remplacent pas les essais sur compte et véhicule réels.

Les prérequis de compilation des cibles Rust sont décrits dans
[Plateformes](docs/plateformes.md). Un workflow CI
préparé n’équivaut pas à des jobs déjà exécutés sur Windows, Linux ou Android.
La [publication automatisée sur crates.io](docs/publication.md) s’exécute sur
les tags de version, après réussite de toutes les vérifications.

## Origine

Protocole et fixtures adaptés de [hacf-fr/renault-api](https://github.com/hacf-fr/renault-api)
version `0.6.3`,
sous licence MIT, copyright 2020 epenet. La notice est conservée dans `LICENSE`.
Cette bibliothèque utilise une API privée pouvant changer côté Renault et n’est
pas affiliée à Renault.

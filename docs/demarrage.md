# Démarrage

[Sommaire](README.md) · [Authentification](authentification.md)

## Prérequis

Rust 1.88 ou supérieur et Cargo sont déclarés dans le manifeste. La compilation
utilise également un compilateur C pour le provider cryptographique `ring`.
Le code livré a été vérifié avec le toolchain installé, pas avec toutes les
versions de Rust à partir de ce minimum déclaré.

La compilation et l’exécution de la crate ne nécessitent ni Python ni service
de stockage. Les fixtures nécessaires aux tests sont incluses dans le paquet.

## Installation depuis crates.io

Après publication de la version `0.1.0`, ajouter à votre `Cargo.toml` :

```toml
[dependencies]
renault-api = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust,no_run
use renault_api::{Auth, Config, RenaultApi};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = std::env::var("RENAULT_SESSION_TOKEN")?;
    let api = RenaultApi::new(Config::new("fr_FR")?)?;
    api.authenticate(Auth::SessionToken(token)).await?;

    for account in api.accounts().await? {
        for vehicle in api.vehicles(&account.id).await? {
            let cockpit = api.cockpit(&account.id, &vehicle.vin).await?;
            println!("{} : {:?} km", vehicle.vin, cockpit.total_mileage_km);
        }
    }
    Ok(())
}
```

La bibliothèque ne crée aucun runtime pour `RenaultApi`. Un exécuteur générique
comme `futures::executor::block_on` ne fournit pas à lui seul les pilotes réseau
et les timers Tokio nécessaires à Reqwest. Le runtime doit être fourni par
l’application appelante.

## Premier essai

1. Fournir `RENAULT_SESSION_TOKEN` avec le mécanisme de configuration de son terminal ou de son IDE.
2. Depuis le dossier de la crate, exécuter `cargo run --example read_vehicle`.
3. Vérifier les comptes et kilométrages retournés, sans supposer que chaque véhicule expose une batterie ou une position.

Les exemples ne sauvegardent pas le jeton et ne le demandent pas à Renault à
partir d’un mot de passe. Pour obtenir une session depuis un compte, suivre le
parcours [email/mot de passe et A2F](authentification.md).

## Appeler une lecture avec un compte et un VIN connus

```rust,no_run
# async fn example() -> renault_api::Result<()> {
use renault_api::{Auth, RenaultApi, Config};

let api = RenaultApi::new(Config::new("fr_FR")?)?;
api.authenticate(Auth::SessionToken("jeton-de-session".into())).await?;
let battery = api.battery_status("account-id", "VF1AAAAAAAAAAAAAA").await?;

if let Some(level) = battery.level_percent {
    println!("Batterie : {level} %");
}
if let Some(updated_at) = battery.updated_at {
    println!("Date du relevé : {}", updated_at.to_rfc3339());
}
# Ok(())
# }
```

`battery_status` peut demander les détails du véhicule avant le relevé pour
connaître son modèle et normaliser les unités. Une valeur `None` indique une
absence de donnée, pas une batterie vide. Un relevé horodaté peut être ancien.

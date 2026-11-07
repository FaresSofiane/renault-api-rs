# Commandes et programmations

[Sommaire](README.md) · [Parité](parite.md)

Les commandes sont des appels explicites. La bibliothèque ne déclenche aucune
action pendant un test de lecture, un chargement de véhicule ou un renouvellement
de jeton. Les tests du dépôt remplacent intégralement le réseau par des mocks.

## Méthodes

Toutes prennent `&self`, `account_id: &str`, `vin: &str` et retournent
`Result<CommandReceipt>` après `.await`.

| Méthode | Paramètres supplémentaires | Effet demandé |
|---|---|---|
| `start_charge` | `when: Option<DateTime<Utc>>` | Démarrer/reprendre la charge |
| `stop_charge` | Aucun | Arrêter/suspendre la charge si disponible |
| `set_charge_mode` | `ChargeModeRequest` | `Always` ou `Scheduled` |
| `set_charge_schedules` | `&[ChargeSchedule]` | Remplacer les programmes KCA/KCM de charge exposés |
| `set_battery_soc` | `min: u8`, `target: u8` | Seuils de charge KCM |
| `start_hvac` | `temperature_celsius: f64`, `when: Option<DateTime<Utc>>` | Démarrer le préconditionnement |
| `stop_hvac` | Aucun | Arrêter le préconditionnement |
| `set_hvac_schedules` | `&[HvacSchedule]` | Remplacer les programmes de préconditionnement |
| `start_horn` | Aucun | Demander le klaxon |
| `start_lights` | Aucun | Demander l’éclairage |
| `refresh_location` | Aucun | Demander un nouveau relevé de position |

## Variantes de charge

| Mode de route | Charge start | Charge stop |
|---|---|---|
| KCA standard | `ChargingStart`, action `start` | Action `stop` lorsque non exclue |
| KCM `charge/start` | Action `start`, date UTC facultative | Dépend de la table du modèle |
| KCM `charge/pause-resume` | Action `resume` | Action `pause` |
| KCM `ev/settings` | Lire les réglages, désactiver tous les programmes, renvoyer le document | Souvent explicitement indisponible |

Pour `ev/settings`, les programmes de charge **et de préconditionnement** sont
désactivés. Chaque champ inconnu et sa représentation JSON sont conservés dans
le document renvoyé. Un document sans tableau `programs` valide est refusé.
La bibliothèque ne restaure pas automatiquement les anciens programmes après
la commande. Ce comportement reproduit le parcours de la référence Python.

Une date différée est acceptée uniquement sur le mode KCM `charge/start` qui
l’expose. Les autres routes refusent une date fournie au lieu de la supprimer.

## Programmations typées

```rust,no_run
use renault_api::{RenaultApi, Result};
use renault_api::models::{ChargeSchedule, ChargeDay, ScheduleTime};

async fn schedule_charge(api: &RenaultApi, account: &str, vin: &str) -> Result<()> {
    let schedule = ChargeSchedule {
        id: Some(1),
        activated: Some(true),
        monday: Some(ChargeDay {
            start_time: Some(ScheduleTime::new(23, 30)?),
            duration_minutes: Some(90),
        }),
        ..Default::default()
    };
    let receipt = api.set_charge_schedules(account, vin, &[schedule]).await?;
    // Conserver l’accusé de réception si l’application souhaite suivre la demande.
    let _ = receipt;
    Ok(())
}
```

`ScheduleTime` utilise une heure UTC `Thh:mmZ`, pas une heure locale avec un
fuseau/DST implicite. `end_time` calcule la fin modulo 24 heures. Chaque semaine
contient les sept jours ; `None` est envoyé comme null pour un jour absent.

Un ID de programme positif et unique est requis à l’envoi. Un jour de charge
présent exige heure et durée entre 1 et 1440 minutes. Un jour HVAC présent exige
`ready_at_time`. Un tableau vide est accepté pour laisser l’API représenter une
suppression de programmation ; le serveur reste responsable de son acceptation.

`ChargingSettings::upsert_schedule` et `HvacSettings::upsert_schedule` ajoutent
ou remplacent un programme complet par ID. Ils ne fusionnent pas un patch JSON
partiel comme certains helpers Python ; modifier d’abord une copie typée de la
programmation existante si l’on souhaite préserver ses autres jours.

## Validation des autres commandes

Le SoC minimal doit être entre 15 et 45, la cible entre 55 et 100, par pas de 5.
Les températures HVAC non finies sont refusées. Aucune plage de température
universelle n’est inventée : les limites effectives restent à la charge du serveur.

Les valeurs de lecture `always`/`scheduled` ne sont pas toujours les chaînes
attendues à l’écriture. `ChargeModeRequest` envoie respectivement
`always_charging` et `schedule_mode`.

L’arrêt HVAC utilise `cancel` ou `stop` selon le mode de route du véhicule.
Une fonction explicitement exclue retourne `UnsupportedEndpoint` avant son POST.

## Accusés de réception et erreurs

`CommandReceipt` contient les identifiants disponibles, le type, le statut, les
attributs structurés et éventuellement les réglages EV retournés. Une réponse
HTTP 204 peut produire un accusé sans identifiant. Les programmes retournés sont
décodés en types de charge/HVAC selon le type de commande.

Un accusé HTTP ne prouve pas l’exécution dans le véhicule. Une perte de réponse
peut laisser l’application sans certitude sur l’état distant. Aucun retry POST
automatique ni mécanisme général de suivi de commande n’est ajouté au protocole.

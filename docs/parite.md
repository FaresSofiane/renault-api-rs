# Parité avec la référence Python

[Sommaire](README.md) · [Référence API](reference-api.md)

Le protocole de [hacf-fr/renault-api](https://github.com/hacf-fr/renault-api)
version `0.6.3` est la référence. Le portage
ne reproduit pas les proxies d’objets ni les noms historiques Python. Les méthodes
Rust prennent explicitement le compte et le VIN et restent asynchrones.

## Lectures et commandes

| Python `RenaultVehicle` | Rust `RenaultApi` |
|---|---|
| `get_details` | `vehicle_details` |
| `get_car_adapter` | `car_adapter` |
| `get_contracts` | `contracts` |
| `get_battery_status` | `battery_status` |
| `get_battery_soc` | `battery_soc` |
| `get_tyre_pressure` | `tyre_pressure` |
| `get_location` | `location` |
| `get_hvac_status` | `hvac_status` |
| `get_hvac_settings` | `hvac_settings` |
| `get_charge_mode` | `charge_mode` |
| `get_charging_settings` | `charging_settings` |
| `get_cockpit` | `cockpit` |
| `get_lock_status` | `lock_status` |
| `get_res_state` | `res_state` |
| `get_charge_schedule` | `charge_schedule` |
| `get_notification_settings` | `notification_settings` |
| `get_charge_history` | `charge_history` |
| `get_charges` | `charges` |
| `get_hvac_history` | `hvac_history` |
| `get_hvac_sessions` | `hvac_sessions` |
| `set_ac_start` / `set_ac_stop` | `start_hvac` / `stop_hvac` |
| `set_battery_soc` | `set_battery_soc` |
| `set_hvac_schedules` | `set_hvac_schedules` |
| `set_charge_schedules` | `set_charge_schedules` |
| `set_charge_mode` | `set_charge_mode` |
| `set_charge_start` / `set_charge_stop` | `start_charge` / `stop_charge` |
| `start_horn` / `start_lights` | `start_horn` / `start_lights` |
| `refresh_location` | `refresh_location` |
| `supports_endpoint` | `supports_endpoint` avec l’enum `Endpoint` |
| `get_endpoint_definition` / `get_full_endpoint` | `endpoint_definition` et sa route typée |
| `http_get` / `http_post` | `http_get<T>` / `http_post<T>` sur l’hôte configuré |

Les comptes et véhicules passent par `accounts` et `vehicles`. Les proxies
`get_api_account`, `get_api_vehicle` et leurs variantes listes sont remplacés par
des structures de données et des identifiants ; aucun appel réseau métier ne
dépend de la conservation de ces classes Python.

Les alertes, présentes comme route dans la table Python, ont aussi une méthode
Rust `alerts` retournant `Vec<Alert>`.

## Authentification et configuration

| Fonction Python | Équivalent Rust |
|---|---|
| Login et exceptions de challenge | `authenticate(Auth::Credentials)` et `AuthOutcome` |
| Jeton de session/restauration | `Auth::SessionToken`, `Auth::RestoredSession`, `session_credentials` |
| Adresses, envoi et vérification A2F | `two_factor_emails`, `request_two_factor_code`, `complete_two_factor` |
| Identifiants d’appareil | `device_ids`, `set_device_ids` |
| JWT/personId à la demande | Gestion interne partagée ; JWT direct accepté également |
| Table locale `get_api_keys` | `Config::new` |
| Configuration chargée depuis Renault | `Config::discover`, explicite et asynchrone |
| Surcharge locale/pays/racines/clés | `Config::with_endpoints` et `Endpoints::new` |
| Helpers de planning et mise à jour | Construction typée, `ScheduleTime`, `upsert_schedule` |

## Différences intentionnelles

- Aucune persistance, CLI, dépendance Home Assistant ou compatibilité avec les anciennes méthodes dépréciées.
- Pas de réécriture des contournements propres à `aiohttp` ; le client réseau est Rust.
- Validation des horaires, seuils SoC, IDs de programmation et intervalles avant envoi des commandes.
- Une charge différée non supportée est refusée ; le Python pouvait ignorer la date dans certaines branches.
- Types, unités explicites et champs d’extension pour préserver les schémas incomplets ou évolutifs.

`get_model_endpoint` conserve la règle Python : une exclusion explicite refuse
la fonction, une entrée absente utilise le défaut. Le fichier généré couvre
28 fonctions pour 25 codes modèles, et une matrice indépendante dans les tests
vérifie chaque combinaison. L’outil d’import lit l’AST Python sans l’exécuter.

## Ce que signifie la vérification

Les tests contrôlent le comportement du code contre les fixtures et les routes
de la référence. Ils ne garantissent ni un serveur Renault inchangé, ni le
support de tous les contrats/véhicules, ni les permissions d’une application
native en production. Les fonctions dont le Python n’établit pas le schéma
conservent des champs JSON d’extension ; leur structure n’est pas inventée.

Le gateway `apis.renault.com`, seulement décrit dans la documentation Python
et non implémenté par son client, n’est pas ajouté comme nouvelle API au portage.

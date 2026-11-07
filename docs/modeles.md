# Modèles de données et unités

[Sommaire](README.md) · [Référence API](reference-api.md)

## Conventions

Les modèles publics sont des structures indépendantes des enveloppes JSON de
Renault. Les noms de champs utilisent `snake_case` et indiquent les unités
lorsqu’elles sont connues. Les nombres à virgule sont des `f64`.

`None` signifie qu’aucune valeur n’a été transmise. `Some(0)` est une valeur
transmise de zéro. Ne pas remplacer systématiquement `None` par zéro dans une
interface ou un calcul.

Les timestamps sont des `chrono::DateTime<chrono::Utc>`. Un offset reçu est
normalisé en UTC. Une date fournie mais invalide provoque `InvalidResponse` ;
une date absente reste `None`. La bibliothèque ne calcule pas de seuil de
fraîcheur et ne déclenche pas de réveil ou de rafraîchissement du véhicule.

Les types de cette page implémentent `Debug`, `Clone`, `PartialEq`,
`Serialize` et `Deserialize`. Les types sans nombres flottants peuvent aussi
implémenter `Eq` ; les enums d’états sont `Copy`. Ces données peuvent contenir
des informations personnelles visibles dans leur `Debug`.

## `Account`

| Champ | Type | Signification |
|---|---|---|
| `id` | `String` | Identifiant de compte Kamereon |
| `account_type` | `Option<String>` | Type de compte brut |
| `status` | `Option<String>` | État du compte brut |

Les chaînes de statut ne sont pas converties en enum fermé : de nouvelles
valeurs serveur restent disponibles.

## `Vehicle`

| Champ | Type | Signification |
|---|---|---|
| `vin` | `String` | Identifiant du véhicule |
| `status` | `Option<String>` | Statut du lien véhicule/compte |
| `details` | `Option<VehicleDetails>` | Détails éventuellement inclus dans le garage |

## `VehicleDetails`

| Champ | Type | Signification |
|---|---|---|
| `vin` | `Option<String>` | VIN dans les détails, éventuellement absent |
| `registration_number` | `Option<String>` | Immatriculation |
| `brand` | `Option<CodeLabel>` | Marque |
| `model` | `Option<CodeLabel>` | Modèle, dont son code de compatibilité |
| `energy` | `Option<CodeLabel>` | Énergie |
| `engine_energy_type` | `Option<String>` | Type d’énergie moteur brut |
| `assets` | `Vec<Asset>` | Ressources fournies par Renault ; vide si absent/null |

`model_code() -> Option<&str>` retourne le code dans `model.code`.
`picture_url() -> Option<&str>` cherche une ressource `PICTURE`, privilégie une
rendition `ONE_MYRENAULT_LARGE`, puis une rendition possédant une URL si aucune
grande rendition n’existe. Une rendition grande mais sans URL peut produire
`None`. La bibliothèque retourne une URL sans télécharger l’image.

## `CodeLabel`, `Asset` et `Rendition`

| Type | Champs |
|---|---|
| `CodeLabel` | `code`, `label`, `group` : `Option<String>` |
| `Asset` | `asset_type`, `role`, `title` : `Option<String>` ; `renditions: Vec<Rendition>` |
| `Rendition` | `resolution_type`, `url` : `Option<String>` |

Le vecteur de renditions est vide lorsque le champ reçu est absent ou null.
Le contenu de `url` dépend du type de ressource ; ne pas supposer que toute
ressource est une image ou une URL HTTPS.

## `BatteryStatus`

| Champ | Type | Source / unité |
|---|---|---|
| `updated_at` | `Option<DateTime<Utc>>` | `timestamp` du relevé |
| `level_percent` | `Option<u8>` | `batteryLevel`, pourcentage de 0 à 100 |
| `temperature_celsius` | `Option<f64>` | `batteryTemperature`, valeur fournie par le véhicule |
| `range_km` | `Option<f64>` | `batteryAutonomy`, kilomètres |
| `plug_status` | `Option<PlugStatus>` | `plugStatus` |
| `charging_status` | `Option<ChargingStatus>` | `chargingStatus` |
| `charging_power_kw` | `Option<f64>` | `chargingInstantaneousPower`, normalisée en kW |
| `charging_remaining_minutes` | `Option<u32>` | `chargingRemainingTime`, minutes |
| `charging_remaining_updated_at` | `Option<DateTime<Utc>>` | Date propre à l’estimation du temps restant |

Pour X101VE (ZOE phase 1), la puissance amont est divisée par 1000. Pour les
autres codes, elle est conservée comme valeur en kW conformément à la référence.
Le modèle est obtenu depuis le garage ou les détails. Avec un modèle inconnu,
le comportement par défaut est de conserver la valeur ; l’unité réelle reste
à confirmer si Renault introduit une nouvelle variante.

Le niveau est refusé s’il sort de 0 à 100. Les valeurs incompatibles avec les
types numériques attendus échouent au décodage. Les autres mesures ne font pas
l’objet d’un modèle physique de plausibilité. Certaines mesures Renault peuvent
être erronées, même après conversion d’unités.

`batteryCapacity`, `batteryAvailableEnergy` et `V2L_SystemStatusDisplay` ne sont
pas exposés par cette V1. Ils ne doivent pas être déduits à partir d’autres
champs sans une règle applicative explicite.

## `PlugStatus`

| Valeur Renault | Variante publique |
|---|---|
| `0` | `Unplugged` |
| `1` | `Plugged` |
| `-1` | `Error` |
| `-2147483648` | `Unavailable` |
| Autre entier | `Unknown(valeur)` |

Le champ absent est `None`. `Some(Unknown(42))` est différent : une valeur
nouvelle a bien été reçue.

## `ChargingStatus`

| Valeur Renault | Variante publique |
|---|---|
| `0.0` | `NotCharging` |
| `0.1` | `WaitingForPlannedCharge` |
| `0.2` | `Ended` |
| `0.3` | `WaitingForCurrentCharge` |
| `0.4` | `EnergyFlapOpen` |
| `1.0` | `Charging` |
| `-1.0` | `ErrorOrNotCharging` |
| `-1.1` | `Unavailable` |
| `-1.3` | `V2gWaiting` |
| `-1.4` | `V2lConnected` |
| `-1.5` | `V2gDischarging` |
| `-1.6` | `V2gCharging` |
| Autre nombre | `Unknown(valeur)` |

Le code `-1.0` est ambigu selon la génération : « pas en charge » sur certaines
anciennes ZOE, « erreur » sur ZE50. La variante publique conserve cette
ambiguïté plutôt que d’inventer une certitude.

Les enums sont `#[non_exhaustive]` : prévoir une branche `_` dans un `match`
extérieur à la crate, en plus des variantes inconnues transportant un code.

## `Cockpit`

| Champ | Type | Unité |
|---|---|---|
| `total_mileage_km` | `Option<f64>` | Kilomètres |
| `fuel_range_km` | `Option<f64>` | Kilomètres |
| `fuel_quantity_liters` | `Option<f64>` | Litres |

Un véhicule électrique peut ne fournir que le kilométrage. Les champs carburant
absents ne représentent pas un réservoir vide.

## `Location`

| Champ | Type | Unité / convention |
|---|---|---|
| `updated_at` | `Option<DateTime<Utc>>` | `lastUpdateTime` |
| `latitude` | `Option<f64>` | Degrés, entre −90 et 90 |
| `longitude` | `Option<f64>` | Degrés, entre −180 et 180 |

Chaque coordonnée est validée lorsqu’elle existe. Le modèle n’impose pas qu’elles
soient présentes ensemble. L’application doit vérifier les deux avant de placer
un point sur une carte. La direction GPS présente dans certaines réponses est
ignorée par la V1.

## JSON public et JSON Renault

La sérialisation Serde des types publics utilise leurs noms Rust : par exemple
`level_percent`, pas `batteryLevel`. Les `Option::None` sont sérialisés en null,
sans règle générale d’omission. Les enums utilisent la représentation Serde
standard ; ils ne sont pas resérialisés automatiquement en codes Renault.

Ces structures ne sont donc pas des corps de requête à renvoyer à Renault.
Le module privé `wire` gère les formes d’entrée et ignore les champs inconnus.
Les enveloppes attendues et les valeurs typées invalides restent contrôlées.

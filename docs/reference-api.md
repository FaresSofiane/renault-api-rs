# Référence de l’API publique

[Sommaire](README.md) · [Modèles](modeles.md) · [Erreurs](erreurs.md)

## Imports

Les types de configuration, d’authentification et d’erreur sont réexportés à la
racine : `Config`, `Endpoints`, `Auth`, `AuthOutcome`, `DeviceIds`,
`SessionCredentials`, `TwoFactorEmail`, `RenaultApi`, `RenaultError`, `Result`,
`Service` et `ApiErrorKind`.

Les types de données se trouvent dans `renault_api::models`.

`Result<T>` est un alias de `std::result::Result<T, RenaultError>`.

## Construction et configuration

| Signature | Comportement |
|---|---|
| `Config::new(locale: &str) -> Result<Config>` | Configuration d’une locale connue |
| `Config::discover(locale: &str).await -> Result<Config>` | Configuration régionale téléchargée explicitement depuis Renault |
| `Endpoints::new(gigya_url: &str, gigya_key: impl Into<String>, kamereon_url: &str, kamereon_key: impl Into<String>) -> Result<Endpoints>` | Racines et clés explicites |
| `Config::with_endpoints(locale: &str, country: &str, endpoints: Endpoints) -> Result<Config>` | Locale/pays/configuration personnalisés |
| `config.with_timeout(timeout: Duration) -> Result<Config>` | Consomme et retourne la configuration modifiée |
| `RenaultApi::new(config: Config) -> Result<RenaultApi>` | Construit le client HTTP sans appel réseau |

Les champs de configuration sont privés. `Config` et `Endpoints` sont clonables.
Le comportement exact de validation est décrit dans [Configuration](configuration.md).

## Authentification : client asynchrone

Toutes les méthodes ci-dessous prennent `&self`.

| Signature après `pub async fn` | Résultat et effets |
|---|---|
| `authenticate(&self, auth: Auth) -> Result<AuthOutcome>` | Remplace la session et peut lancer un challenge A2F |
| `two_factor_emails(&self) -> Result<Vec<TwoFactorEmail>>` | Prépare le challenge si nécessaire et lit les adresses |
| `request_two_factor_code(&self, email_id: Option<&str>) -> Result<()>` | Envoie le code ; le choix est obligatoire si plusieurs adresses existent |
| `complete_two_factor(&self, code: &str, remember_device: bool) -> Result<AuthOutcome>` | Vérifie, finalise et refait le login |
| `set_device_ids(&self, device_ids: DeviceIds) -> Result<()>` | Valide/restaure les cookies hors challenge |
| `session_credentials(&self) -> Option<SessionCredentials>` | Copie explicite de la session exportable, sans réseau |
| `device_ids(&self) -> DeviceIds` | Copie des identifiants d’appareil, sans réseau |
| `cancel_two_factor(&self)` | Efface le challenge, sans réseau |
| `logout(&self)` | Efface l’état partagé, sans requête de révocation |

## Lectures

Les méthodes sont asynchrones. Toutes prennent `&self` et retournent `Result<T>`.

| Méthode | Paramètres | `T` |
|---|---|---|
| `accounts` | Aucun | `Vec<Account>` |
| `vehicles` | `account_id: &str` | `Vec<Vehicle>` |
| `vehicle_details` | `account_id: &str, vin: &str` | `VehicleDetails` |
| `battery_status` | `account_id: &str, vin: &str` | `BatteryStatus` |
| `cockpit` | `account_id: &str, vin: &str` | `Cockpit` |
| `location` | `account_id: &str, vin: &str` | `Location` |

### `accounts`

Détermine `personId` depuis le JWT ou, avec un jeton de session, depuis Gigya
`accounts.getAccountInfo`. Lit ensuite les comptes Kamereon. Les entrées sans
`accountId` non vide sont filtrées. L’absence du tableau `accounts` ou un tableau
de structure incorrecte produit `InvalidResponse`.

Un JWT sans `personId` et sans jeton de session ne permet pas cette découverte.
La méthode retourne `InvalidInput` ; les lectures avec identifiants explicites
peuvent rester disponibles.

### `vehicles`

Lit le garage du compte. Les liens sans VIN non vide sont filtrés. Les détails
peuvent être absents. Lorsqu’ils existent, leurs codes modèles sont mémorisés
pour les unités et les routes ; aucune télémétrie n’est conservée.

### `vehicle_details`

Demande toujours les détails au serveur et actualise le code modèle mémorisé.
Le numéro d’immatriculation et les ressources sont facultatifs. Le code radio est exposé lorsqu’il est présent dans la réponse.

### `battery_status`

Obtient le modèle si nécessaire, bloque les cas connus comme indisponibles et
lit `battery-status` version 2. Normalise les watts X101VE en kW. Les dates et
les états sont convertis en types publics ; les valeurs absentes restent `None`.

### `cockpit`

Lit `cockpit` version 1, sans demander les détails préalablement. Expose le
kilométrage, l’autonomie carburant et la quantité de carburant si disponibles.

### `location`

Obtient le modèle si nécessaire, refuse les modèles explicitement exclus dans
la table de compatibilité actuelle et lit `location` version 1. Valide les
bornes des coordonnées lorsque celles-ci sont présentes.

## Appels HTTP implémentés

Les chemins suivants sont ajoutés à la racine Kamereon configurée.
Tous les GET ci-dessous ajoutent `country=<pays>`.

| Lecture | Chemin |
|---|---|
| Comptes | `/commerce/v1/persons/{person_id}` |
| Véhicules | `/commerce/v1/accounts/{account_id}/vehicles` |
| Détails | `/commerce/v1/accounts/{account_id}/vehicles/{vin}/details` |
| Batterie | `/commerce/v1/accounts/{account_id}/kamereon/kca/car-adapter/v2/cars/{vin}/battery-status` |
| Cockpit | `/commerce/v1/accounts/{account_id}/kamereon/kca/car-adapter/v1/cars/{vin}/cockpit` |
| Position | `/commerce/v1/accounts/{account_id}/kamereon/kca/car-adapter/v1/cars/{vin}/location` |

Les headers sont `content-type: application/vnd.api+json`, `apikey` et
`x-gigya-id_token`. Les identifiants sont ajoutés comme segments d’URL encodés,
sans concaténation permettant de changer la route. Les chaînes vides, `.`/`..`
et les caractères de contrôle sont refusés ; le VIN n’est pas limité localement
à une validation complète de son format industriel.

## Protocole Gigya

| Endpoint | Méthode | Rôle |
|---|---|---|
| `accounts.login` | POST formulaire | Identifiants vers jeton de session, ou challenge |
| `accounts.getJWT` | POST formulaire | JWT avec durée demandée de 900 secondes |
| `accounts.getAccountInfo` | POST formulaire | `data.personId` si nécessaire |
| `accounts.webSdkBootstrap` | GET | Acquisition des cookies `gmid`/`ucid` |
| `accounts.tfa.initTFA` | GET | Assertion du challenge email |
| `accounts.tfa.email.getEmails` | GET | Adresses éligibles |
| `accounts.tfa.email.sendVerificationCode` | GET | Envoi du code |
| `accounts.tfa.email.completeVerification` | GET | Validation du code |
| `accounts.tfa.finalizeTFA` | GET | Finalisation avant nouveau login |

La clé est envoyée comme `ApiKey` dans les formulaires POST et `APIKey` dans
les paramètres GET. Les cookies d’appareil sont envoyés explicitement sur les
appels A2F et le login concerné ; la bibliothèque n’utilise pas de cookie jar
global. Les méthodes réseau bas niveau restent privées.

## Lectures supplémentaires

Toutes les méthodes suivantes prennent le compte et le VIN. Elles retournent
`Result<T>` et nécessitent `.await`.

| Méthode | `T` |
|---|---|
| `car_adapter` | `CarAdapter` |
| `contracts` | `Vec<Contract>` |
| `alerts` | `Vec<Alert>` |
| `battery_soc` | `BatterySoc` |
| `tyre_pressure` | `TyrePressure` |
| `hvac_status` | `HvacStatus` |
| `hvac_settings` | `HvacSettings` |
| `charge_mode` | `ChargeMode` |
| `charging_settings` | `ChargingSettings` |
| `lock_status` | `LockStatus` |
| `res_state` | `ResState` |
| `charge_schedule` | `ChargeScheduleData` |
| `notification_settings` | `NotificationSettings` |

`charge_schedule` distingue `Calendar` (KCA) et `EvSettings` (KCM).
`battery_soc` lit une réponse KCM directe, sans enveloppe `data.attributes`.
`lock_status` absorbe les variantes `doorStatus*` et `statusDoor*` ; lorsqu’elles
coexistent, la valeur `doorStatus*` non null est prioritaire.

`contracts` accepte le tableau direct de contrats et une enveloppe `contractList`.
Les paramètres incluent locale, marque Renault et les trois catégories de
contrats exposées par la référence. Les alertes acceptent un tableau direct ou
un tableau `data`.

### Historiques

`charge_history` et `hvac_history` prennent ensuite `start: NaiveDate`,
`end: NaiveDate` et `period: HistoryPeriod`. Les résultats sont respectivement
`ChargeHistory` et `HvacHistory`.

`charges` et `hvac_sessions` prennent `start: NaiveDate` et `end: NaiveDate`.
Les résultats sont `Charges` et `HvacSessions`.

La borne de début doit précéder ou égaler la borne de fin. Le format des dates
est `YYYYMMDD` pour les jours et `YYYYMM` pour les agrégats mensuels. `type`
vaut `day` ou `month` sur les agrégats. Les durées historiques brutes conservent
une indication d’unité issue des règles du modèle, sans prétendre corriger une
mesure incohérente côté serveur.

### Support et routes

`endpoint_definition(account_id, vin, endpoint: Endpoint)` retourne une
`EndpointDefinition` contenant le chemin relatif de l’adaptateur et son mode.
`supports_endpoint` retourne `Result<bool>` ; une route par défaut pour un
modèle inconnu ne prouve pas le support réel par le véhicule.

`Endpoint::ALL` énumère les 28 fonctions. Sa méthode pure
`definition_for_model(Option<&str>)` permet de consulter la même table sans
réseau. Les exclusions et modes KCA/KCM reproduisent les 25 tables Python.

### HTTP bas niveau

`http_get<T: DeserializeOwned>(endpoint: &str)` et
`http_post<T: DeserializeOwned>(endpoint: &str, body: &serde_json::Value)`
retournent `Result<T>`. Le chemin doit commencer par `/`, rester sur l’hôte
configuré, ne contenir ni fragment ni backslash et ne pas remplacer le pays
configuré. Les query strings sont acceptées. Les URLs absolues et `//host`
sont refusés avant envoi. Le POST n’est jamais rejoué automatiquement.

## Commandes

Les signatures et conséquences sont détaillées dans [Commandes](commandes.md).
Toutes retournent un `CommandReceipt` typé, avec extensions pour les attributs
non établis. Les routes d’actions KCA, KCM start, pause/resume et `ev/settings`
sont implémentées.

Le gateway `apis.renault.com`, décrit mais non implémenté dans le client Python,
n’est pas inclus. CLI, stockage persistant et proxies dépréciés sont exclus.

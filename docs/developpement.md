# Développement, tests et maintenance

[Sommaire](README.md) · [Architecture](architecture.md)

## Commandes de référence

Depuis la racine de la crate :

```sh
cargo test --locked
cargo check --examples --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
```

`--locked` impose le `Cargo.lock` versionné. Ajouter `--offline` si les
dépendances sont déjà présentes et que l’on souhaite interdire leur
téléchargement. Les premières compilations peuvent nécessiter l’accès au
registre Cargo.

Les tests ouvrent des ports sur loopback avec Wiremock. Un environnement qui
interdit l’ouverture de sockets locales peut les refuser avec `PermissionDenied`.
Ce refus n’indique pas à lui seul une régression de la bibliothèque.

## Vérifier la documentation

```sh
cargo test --doc --locked
cargo doc --no-deps --locked
```

Les blocs Rust des guides sont inclus dans la collecte de doctests. Les blocs
`no_run` sont compilés sans être exécutés : aucun compte réel ni envoi de code
A2F n’est nécessaire. 

Les exemples exécutables se vérifient avec `cargo check --examples`.
Les lancer avec `cargo run` et un vrai jeton est une action distincte qui contacte
Renault. Ne pas faire dépendre la suite de tests d’un compte ou d’un véhicule réel.

## Répartition des tests

| Fichier | Couverture |
|---|---|
| `tests/authentication.rs` | Identifiants, A2F, code incorrect, sélection email, reprise, session, JWT, concurrence et masquage des erreurs |
| `tests/vehicle_data.rs` | Fixtures véhicules/télémétrie, unités, compatibilité, routes, headers, enveloppes et erreurs |
| `tests/configuration.rs` | Locales, racines, paramètres, cookies et traits du client |
| `tests/common/mod.rs` | Serveurs et données auxiliaires de test |

Les JWT synthétiques servent à l’inspection des claims et aux requêtes mockées.
Ils ne sont pas signés pour être acceptés par Renault et ne testent pas une
validation cryptographique locale, que la crate n’effectue pas.

## Fixtures

Les réponses JSON copiées de la référence Python sont dans `tests/fixtures`.
Elles restent à l’intérieur de la crate pour permettre de déplacer ou empaqueter
le dossier sans dépendre de la copie Python voisine.

Les identifiants et données personnelles doivent être anonymisés avant d’ajouter
une fixture. Conserver la structure, les types et les particularités utiles au
test ; remplacer VIN, immatriculation, coordonnées et identifiants personnels
par des données fictives cohérentes. Ne pas conserver de jetons utilisables.

Les tests qui parcourent les répertoires de véhicules et de télémétrie vérifient
les fichiers présents. Ajouter un test ciblé lorsqu’une fixture introduit une
nouvelle règle métier, pas uniquement un nouveau champ ignoré.

## Ajouter une lecture

1. Identifier la route, la version, les headers et les particularités du modèle dans la référence et ses fixtures.
2. Définir une structure d’entrée privée dans `wire.rs` et un type public dans `models.rs` avec unités et valeurs absentes explicites.
3. Ajouter la méthode dans `RenaultApi`, avec sa signature asynchrone et son type de retour.
4. Tester le décodage, les paramètres réseau et les cas d’erreur utiles avec un serveur local.
5. Mettre à jour la référence API, les modèles, le périmètre du README et les exemples concernés.

Ne pas exposer une méthode HTTP générique uniquement pour éviter de définir un
contrat public. Ne pas réutiliser une structure de réponse publique comme corps
d’action sans valider sa correspondance réelle au protocole Renault.

## Ajouter ou corriger une variante de véhicule

Pour une lecture existante, vérifier si la différence concerne l’endpoint, les
unités, les champs ou les services disponibles. Les exclusions actuelles sont
dans `client.rs` ; la conversion X101VE est dans `wire.rs`.

Un modèle absent des exclusions utilise la route standard, ce qui n’est pas une
preuve de support. Ajouter la fixture et un test démontrant la nouvelle règle.
Une capacité peut aussi dépendre du contrat et ne doit pas être promise pour
tous les véhicules possédant le même code modèle.

## Sécurité et compatibilité

Ne pas ajouter de logging des corps Gigya ou des query strings A2F. Les variantes
d’erreur doivent rester exploitables sans afficher les secrets. L’export de
session reste explicite ; ne pas lui ajouter de sauvegarde automatique.

Le cœur interdit le code `unsafe`. Cela ne signifie pas que toutes ses
dépendances en sont dépourvues. Maintenir Rustls, Reqwest, `ring` et les racines
de certificats à jour, puis revalider les cibles nécessaires au produit.

Les champs et signatures publics constituent le contrat de l’application.
Modifier un nom, une unité ou une représentation Serde peut affecter les
consommateurs même si le compilateur accepte la modification à l’intérieur
de cette crate. Documenter les changements lors de futures versions.

## Conditionnement

```sh
cargo package --list --allow-dirty
```

Cette commande permet d’inspecter les fichiers destinés au paquet ; elle ne
publie rien. Le dossier `target` est ignoré. Conserver sources, guides, exemples,
fixtures nécessaires et licence d’origine dans un livrable autonome.

Le workflow CI inclut des vérifications desktop et mobiles. Les détails et
limites de chaque cible sont dans [Plateformes](plateformes.md).

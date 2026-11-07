# Plateformes et intégration native

[Sommaire](README.md) · [Architecture](architecture.md)

## Portée

La crate fournit une API Rust utilisable dans un binaire ou une bibliothèque
Rust. Elle ne livre ni XCFramework, ni AAR Android, ni ABI C, ni bindings
Swift/Kotlin/C#. Les construire reste une couche d’intégration indépendante.

Compiler la crate pour une plateforme vérifie son code et ses dépendances ;
cela ne prouve pas qu’une application native peut déjà l’importer dans son
langage, ni que le réseau a été testé sur un appareil de cette plateforme.

## Matrice de vérification

| Cible | Vérification locale déjà effectuée lors du portage initial | Workflow fourni |
|---|---|---|
| macOS ARM64 | Tests natifs et compilation du cœur | Tests sur le runner macOS |
| macOS x86_64 | `cargo check` | Pas de job Intel distinct |
| iOS ARM64 | `cargo check` et `cargo build` du cœur | `cargo check` |
| Simulateur iOS ARM64 | `cargo check` du cœur | `cargo check` |
| Windows | Non exécutée ici | Tests et Clippy natifs |
| Linux | Non exécutée ici | Tests et Clippy natifs |
| Android ARM64/x86_64 | Non exécutée ici | `cargo check` avec NDK |

Les validations locales ne sont pas des tests de fonctionnement sur un compte
Renault réel et ne garantissent pas chaque architecture/version de système.

## macOS

Sur une machine munie de Xcode ou des outils de développement Apple :

```sh
cargo test --locked
cargo check --locked --target aarch64-apple-darwin
cargo check --locked --target x86_64-apple-darwin
```

Installer préalablement les cibles manquantes avec `rustup target add`.
Le provider `ring` a besoin du compilateur et des outils Apple appropriés.
L’intégration doit exposer des appels asynchrones adaptés à une application
SwiftUI/AppKit.

## iOS

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cargo check --locked --target aarch64-apple-ios
cargo check --locked --target aarch64-apple-ios-sim
cargo build --locked --target aarch64-apple-ios
```

Ces commandes supposent macOS, Xcode et les SDK iOS installés. Elles ne créent
pas à elles seules un module Swift. Une couche UniFFI ou FFI devra décrire
l’interface exportée, convertir les types et conditionner les architectures.

Une future application iOS doit gérer sa propre durée de vie et ses tâches
d’arrière-plan. Le client ne maintient pas le processus actif lorsque le système
suspend l’application et n’installe pas de service de polling.

## Windows

Utiliser un toolchain Rust natif, par exemple `x86_64-pc-windows-msvc`, avec les
outils C/C++ correspondants disponibles pour `ring`.

```sh
cargo test --locked
cargo check --examples --locked
```

Le workflow utilise un runner Windows natif ; il ne valide pas toutes les
combinaisons MSVC/GNU ou ARM64. Aucun binding .NET n’est inclus.

## Linux

Le cœur ne requiert pas OpenSSL installé. La compilation nécessite tout de même
un compilateur C et les outils du système cible pour le provider cryptographique.
Les paquets exacts dépendent de la distribution et de l’architecture.

```sh
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

Le job Linux standard ne garantit pas la compatibilité musl, les anciennes
versions de glibc ou les architectures non testées. Ajouter les jobs adaptés à
son produit si ces cibles sont requises.

## Android

La compilation croisée nécessite les bibliothèques standard Rust cibles et un
Android NDK compatible. Depuis un environnement Linux avec le NDK configuré :

```sh
rustup target add aarch64-linux-android x86_64-linux-android
```

Le workflow définit `CC_aarch64_linux_android`, `AR_aarch64_linux_android`,
`CC_x86_64_linux_android` et `AR_x86_64_linux_android` vers les compilateurs et
`llvm-ar` du NDK, puis lance :

```sh
cargo check --locked --target aarch64-linux-android
cargo check --locked --target x86_64-linux-android
```

Les chemins de NDK varient entre machines ; reprendre les variables de son
installation plutôt que copier un chemin absolu d’un autre poste.

Le client préconfigure Rustls avec les racines Mozilla. Il ne fait pas appel au
vérificateur de certificats JVM Android pour établir ses connexions. Cela ne
crée pas un binding JNI et ne remplace pas la configuration Android de
l’application : celle-ci doit notamment déclarer l’accès réseau nécessaire et
intégrer ses bibliothèques natives dans les ABI qu’elle distribue.

## Interface depuis du code natif

Une future couche de binding doit piloter explicitement le runtime Tokio et
exposer des opérations asynchrones autour de `RenaultApi`. La crate ne fournit
pas de runtime intégré ou de façade synchrone.

## CI autonome

Le workflow est dans [`.github/workflows/ci.yml`](../.github/workflows/ci.yml).
Il est conçu pour un dépôt où cette crate se trouve à la racine. Tant qu’elle
reste dans un sous-dossier d’un autre dépôt, GitHub ne découvre pas ce workflow
imbriqué automatiquement. Le déplacer à la racine du futur dépôt indépendant,
ou adapter explicitement le workflow du dépôt hôte.

Ne pas confondre « workflow préparé » et « jobs exécutés avec succès » : les
résultats Windows/Linux/Android devront être observés sur le dépôt où il tourne.

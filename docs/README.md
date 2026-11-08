# Documentation de Renault API Rust

Cette documentation décrit le client asynchrone `RenaultApi` livré dans cette
crate. Il expose les lectures, commandes, historiques, contrats et l’authentification repris de la référence Python.

## Parcours de lecture

| Besoin                                                                     | Document                                       |
|----------------------------------------------------------------------------|------------------------------------------------|
| Installer et faire un premier appel asynchrone | [Démarrage](demarrage.md)                      |
| Comprendre les identifiants, les jetons et l’A2F email                     | [Authentification](authentification.md)        |
| Trouver une méthode, sa signature ou son endpoint                          | [Référence API](reference-api.md)              |
| Connaître tous les champs, unités et états                                 | [Modèles de données](modeles.md)               |
| Choisir locale, pays, région, URL et timeout                               | [Configuration](configuration.md)              |
| Traiter une erreur et diagnostiquer un appel                               | [Erreurs et dépannage](erreurs.md)             |
| Comprendre les runtimes, clones, caches et limites                         | [Architecture et concurrence](architecture.md) |
| Préparer macOS, Windows, Linux, Android et iOS                             | [Plateformes](plateformes.md)                  |
| Compiler, tester, maintenir et étendre la crate                            | [Développement](developpement.md)              |
| Configurer le secret GitHub et publier une version sur crates.io          | [Publication](publication.md)                  |

Voir aussi la [matrice de parité](parite.md) et le [guide des commandes](commandes.md).

## Exécution du client

`RenaultApi` expose des méthodes `async` à utiliser avec `.await`. Le réseau
requiert un runtime Tokio fourni par l’application. La bibliothèque ne crée
pas de runtime ni de thread de service. Ses clones partagent la session et
le pool HTTP ; les requêtes peuvent se chevaucher et les échanges
d’authentification sont sérialisés.

## Ce que fournit la bibliothèque

Un client accepte un mode d’authentification, découvre les comptes et véhicules,
puis retourne des structures Rust. Il gère le protocole Gigya/Kamereon, les
jetons, les erreurs et les différences connues des lectures selon les modèles.

L’application garde la responsabilité de l’interface, de la sauvegarde éventuelle
des sessions, du rythme des appels et de leur exécution hors du thread principal
d’une interface native. La bibliothèque ne lit pas les variables d’environnement ;
seuls les exemples le font pour recevoir un jeton sans l’inscrire dans le code.

## Exemples exécutables

| Fichier                                                            | Commande depuis la racine de la crate                           |
|--------------------------------------------------------------------|-----------------------------------------------------------------|
| [`read_vehicle.rs`](../examples/read_vehicle.rs)                   | `cargo run --example read_vehicle`                              |

Ces commandes font de vrais appels Renault si `RENAULT_SESSION_TOKEN` est fourni.
Les commandes de test de cette documentation n’utilisent que des serveurs locaux.
Ne pas publier le jeton, les traces contenant des données personnelles ou les VIN.

## État de la compatibilité

La bibliothèque expose uniquement une API Rust. Les vérifications
locales Apple et le workflow CI sont détaillés dans [Plateformes](plateformes.md).
Une cible compilable ne signifie pas que l’authentification et le réseau ont été
testés sur un appareil réel.

Le protocole est adapté de `hacf-fr/renault-api` version `0.6.3`. Il s’agit d’une API
privée pouvant changer côté Renault. La notice MIT d’origine est conservée dans
[`LICENSE`](../LICENSE).

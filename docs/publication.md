# Publication sur crates.io

[Sommaire](README.md) · [Développement](developpement.md)

Le workflow [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) vérifie la
crate sur les pushes et les pull requests. Un push de tag `v*` déclenche aussi
la publication lorsque tous les jobs de vérification réussissent.

## Configuration initiale

1. Se connecter à [crates.io](https://crates.io) et vérifier l’adresse email du compte.
2. Créer un [jeton API](https://crates.io/settings/tokens) autorisant la publication de `renault-api`. Pour une première publication, choisir les autorisations permettant de créer cette crate ; pour les suivantes, le compte doit en être propriétaire.
3. Dans les paramètres du dépôt GitHub, ouvrir **Settings → Secrets and variables → Actions → New repository secret** et enregistrer le jeton sous le nom `CARGO_REGISTRY_TOKEN`.

Le jeton est fourni uniquement à l’étape de publication. Les tests et la
vérification du paquet s’exécutent sans ce secret, y compris dans les pull
requests. Il n’est pas nécessaire d’exécuter `cargo login` dans le workflow.

La création du secret est décrite dans la
[documentation GitHub](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-secrets).
Cargo lit le jeton via la variable d’environnement
[`CARGO_REGISTRY_TOKEN`](https://doc.rust-lang.org/cargo/reference/config.html#registrytoken).

## Publier une version

Depuis la racine du dépôt de la crate :

1. Définir la version dans `Cargo.toml` et actualiser `Cargo.lock` avec `cargo check` si nécessaire.
2. Commiter les changements et pousser le commit sur GitHub.
3. Créer et pousser le tag correspondant à la version du manifeste. Pour `0.1.0` :

   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```

Le tag doit correspondre exactement à `v` suivi de `package.version`.
Le workflow refuse par exemple `v0.2.0` si le manifeste contient `0.1.0`.

## Vérifications avant l’envoi

| Job | Vérification |
|---|---|
| `package` | Correspondance du tag et `cargo publish --dry-run --locked --registry crates-io` |
| `desktop` | Tests, exemples, formatage et Clippy sur Linux, Windows et macOS |
| `ios` | Compilation pour iOS et le simulateur ARM64 |
| `android` | Compilation ARM64 et x86_64 avec le NDK |
| `publish` | Publication après réussite des quatre jobs précédents |

Les pushes de branches et les pull requests exécutent les vérifications sans
publier. Les publications sont sérialisées par un groupe de concurrence.

Le mode `--dry-run` fabrique et compile le paquet sans l’envoyer. La publication
réelle utilise `cargo publish --locked --registry crates-io`, conformément à la
[documentation Cargo](https://doc.rust-lang.org/cargo/reference/publishing.html).

## Échec ou nouvelle version

Si un job de vérification échoue, `publish` ne s’exécute pas. Si le secret est
absent, l’étape de publication s’arrête avec un message indiquant son nom.
Le registre vérifie aussi la disponibilité du nom et les droits du compte.

Une version déjà publiée ne peut pas être remplacée. Après une correction,
augmenter la version du manifeste, actualiser le lockfile, commiter et pousser
un nouveau tag. Si l’envoi a réussi mais que le workflow a ensuite signalé un
échec, vérifier la version sur crates.io avant de relancer la publication.

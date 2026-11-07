# Configuration

[Sommaire](README.md) · [Plateformes](plateformes.md)

## Locale et pays

`Config::new("fr_FR")` détermine la configuration incluse pour cette locale et
déduit le pays `FR`. Le format accepté est exactement deux lettres ASCII
minuscules, `_`, puis deux lettres ASCII majuscules : `ll_CC`.

`fr-FR`, `FR_fr`, une locale vide et un code comportant une lettre accentuée
sont refusés. Une locale bien formée mais inconnue est également refusée par
`Config::new` : la bibliothèque ne choisit pas silencieusement une région.

## Locales incluses

| Région | Locales |
|---|---|
| Europe, clé Gigya commune | `bg_BG`, `cs_CZ`, `da_DK`, `de_DE`, `de_AT`, `de_CH`, `en_GB`, `en_IE`, `es_ES`, `fi_FI`, `fr_FR`, `fr_BE`, `fr_CH`, `hr_HR`, `hu_HU`, `it_IT`, `it_CH`, `nl_NL`, `nl_BE`, `no_NO`, `pl_PL`, `pt_PT`, `ro_RO`, `ru_RU`, `sk_SK`, `sl_SI`, `sv_SE` |
| Europe, clé Gigya distincte | `fr_LU` |
| Australie | `en_AU` |
| Amérique, configuration du Mexique | `es_MX` |

Cette liste reproduit la configuration de la référence Python, pas une liste
de marchés dont le fonctionnement aurait été vérifié sur des comptes réels.

`Config::discover(locale).await` télécharge explicitement le document public
`configuration/android/config_<locale>.json` utilisé par la référence Python.
Cette méthode accepte une nouvelle locale bien formée et permet de rafraîchir
les clés/racines publiques sans modifier la table embarquée. Elle requiert Tokio
et n’effectue aucune sauvegarde. `Config::new` reste entièrement local.

Les getters `locale`, `country`, `endpoints` et `timeout`, ainsi que les getters
des racines/clés sur `Endpoints`, permettent de consulter la configuration.

## Racines régionales

| Région | Gigya | Kamereon |
|---|---|---|
| Europe | `https://accounts.eu1.gigya.com` | `https://api-wired-prod-1-euw1.wrd-aws.com` |
| Australie | `https://gigya-prod-au1.renaultgroup.com` | `https://api-wired-prod-1-apn1.wrd-aws.com` |
| Mexique | `https://accounts.us1.gigya.com` | `https://api-wired-prod-1-usw2.wrd-aws.com` |

Les clés embarquées sont des clés publiques d’application issues de la référence.
Elles sont distinctes des jetons et mots de passe de l’utilisateur. Si Renault
les modifie, une configuration explicite permet de les remplacer sans changer
les signatures du client.

## Endpoints et pays explicites

```rust,no_run
use renault_api::{Config, Endpoints, Result};

fn custom_config() -> Result<Config> {
    let endpoints = Endpoints::new(
        "https://accounts.eu1.gigya.com",
        "cle-publique-gigya",
        "https://api-wired-prod-1-euw1.wrd-aws.com",
        "cle-publique-kamereon",
    )?;
    Config::with_endpoints("fr_FR", "FR", endpoints)
}
```

Les clés de cet extrait sont des placeholders à remplacer, pas des clés valides.
`with_endpoints` valide la syntaxe de la locale et du pays, mais n’exige pas que
la locale soit dans la table incluse. Cela permet une région ou une configuration
nouvelle. L’application doit fournir la configuration correspondant réellement
au compte concerné.

Le pays doit être deux lettres ASCII majuscules. Il est transmis aux lectures
Kamereon. La langue des emails A2F est dérivée des deux premières lettres de la
locale ; le pays explicite n’en change pas la langue.

## Validation des URL

Les racines exigent HTTPS. HTTP est accepté uniquement pour `localhost`,
`127.0.0.1` et `[::1]`, afin d’exécuter des serveurs de test locaux.

Une URL contenant un identifiant/mot de passe, une query string ou un fragment
est refusée. Les clés API ne doivent pas être placées dans l’URL racine.
Les clés vides sont refusées. La validation locale n’atteste pas qu’une clé
non vide soit acceptée par Renault.

Un préfixe de chemin dans la racine est conservé et les segments sont ajoutés
après lui. Le client ne suit pas les redirections, y compris vers une autre
racine. Il ne propose pas de réglage public pour désactiver la validation TLS.

## Délais

```rust,no_run
use std::time::Duration;
use renault_api::{Config, Result};

fn config() -> Result<Config> {
    Config::new("fr_FR")?.with_timeout(Duration::from_secs(15))
}
```

| Réglage | Valeur |
|---|---|
| Timeout par requête HTTP | 30 secondes par défaut |
| Timeout de connexion | Le plus petit de 10 secondes et du timeout configuré |
| Timeout nul | Refusé |
| Limite d’un corps JSON lu | 2 MiB |
| Retries HTTP automatiques | Désactivés |

Le timeout HTTP n’est pas un délai global par méthode. Une lecture peut d’abord
obtenir un JWT et des détails, puis faire sa requête principale. L’attente du
verrou d’authentification n’est pas incluse dans
le timeout d’une requête.

Il n’existe pas en V1 de paramètre public pour changer la marge JWT de 60 secondes,
la limite de réponse, le provider TLS, le user agent, le proxy ou la politique de
redirection. Les headers d’authentification sont assemblés par la bibliothèque.

## Certificats

TLS utilise Rustls, `ring` et les racines Mozilla embarquées par `webpki-roots`.
Le client ne dépend pas de la présence d’OpenSSL sur l’ordinateur. Il n’utilise
pas le magasin de certificats de l’appareil : un certificat privé ajouté par
l’utilisateur ou l’entreprise ne sera pas automatiquement accepté.

Maintenir les dépendances à jour pour actualiser les racines et les correctifs
réseau. Une interception HTTPS par un proxy d’entreprise peut échouer si son
autorité n’appartient pas aux racines embarquées ; la V1 n’expose pas l’ajout
d’une autorité personnalisée.

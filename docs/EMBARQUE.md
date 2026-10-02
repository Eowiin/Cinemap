# Rust embarqué : pistes pour plus tard

Notes perso, sans lien direct avec Cinemap. Ce que j'apprends ici (emprunts, itérateurs, pas d'allocation cachée) sert directement sur microcontrôleur.

## Ce qui change par rapport à Cinemap

- **`#![no_std]`** : pas de bibliothèque standard, donc pas de fichiers, de threads, de réseau « tout fait », et **pas d'allocateur par défaut** (pas de `String`, `Vec`, `Box`). Il reste `core` : types de base, itérateurs, `Option`/`Result`, slices, formatage.
- **Mémoire de taille fixe**, connue à la compilation. Le crate [`heapless`](https://docs.rs/heapless) fournit `Vec<T, N>`, `String<N>`, des files, etc., sans allocation.
- **Pas de `panic` affiché** : on choisit un comportement (`panic-halt`, `panic-probe` qui envoie le message au PC via le débogueur).
- **Accès au matériel** en couches : PAC (registres bruts) → HAL (API sûre pour une puce, ex. `rp2040-hal`, `esp-hal`) → traits génériques [`embedded-hal`](https://docs.rs/embedded-hal) (un driver de capteur écrit une fois marche sur toutes les puces).
- **Async sans OS** : [Embassy](https://embassy.dev), un exécuteur async pour microcontrôleurs. Le `async`/`.await` appris avec tokio se retrouve presque tel quel.

## Par où commencer

1. [The Embedded Rust Book](https://docs.rust-embedded.org/book/) : les bases (`no_std`, registres, interruptions).
2. [Discovery Book](https://docs.rust-embedded.org/discovery/) : tutoriel pratique sur une carte micro:bit.
3. [Embassy book](https://embassy.dev/book/) : quand les bases sont là.
4. [Awesome Embedded Rust](https://github.com/rust-embedded/awesome-embedded-rust) : liste de crates et de drivers.

## Matériel pour débuter (quelques euros)

| Carte | Pourquoi |
|---|---|
| **Raspberry Pi Pico / Pico 2** (RP2040 / RP2350) | Très bien supportée en Rust (`rp-hal`, Embassy), pas chère, bien documentée |
| **ESP32-C3** (RISC-V) | Wi-Fi + Bluetooth, support Rust officiel d'Espressif (`esp-hal`) |
| **micro:bit v2** | Celle du Discovery Book, LED et capteurs intégrés |

Outils : [`probe-rs`](https://probe.rs) pour flasher et déboguer, `defmt` pour des logs très légers.

## Idées de premiers projets

- Faire clignoter une LED, puis la piloter par un bouton (interruptions).
- Lire un capteur de température (I2C) et afficher la valeur via `defmt`.
- Avec l'ESP32-C3 : un petit afficheur qui interroge l'API Cinemap et montre les prochaines séances du cinéma d'à côté.

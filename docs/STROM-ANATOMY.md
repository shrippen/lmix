# Strom-Anatomie

Für uns verständlich aufbereitet, aus dem gelesenen Code
(`workspace/strom/backend/src/`, Stand `main` bei Submodule-Hinzunahme). Code
ist Quelle der Wahrheit — bei Zweifel im Code nachsehen, nicht hier.

## Flow-Lifecycle

Ein **Flow** (`strom_types::Flow`) ist eine benannte, persistierbare
GStreamer-Pipeline-Definition: `id`, `name`, `elements[]` (rohe GStreamer-
Elemente), `blocks[]` (höherwertige Bausteine, siehe unten), `links[]`
(Pad-Verbindungen), `running: bool`, `gst_state` (rohe Pipeline-State für
Diagnose).

REST-Zyklus (verifiziert per `curl` gegen `POST/PUT/DELETE /api/flows[/{id}]`):

1. `POST /api/flows` mit `{id, name}` (id ist eine client-generierte UUID,
   Pflichtfeld) — legt einen leeren Flow an.
2. `PUT /api/flows/{id}` mit vollständigem Flow-Objekt (inkl. `elements`)
   — ersetzt Elemente/Blocks/Links. Elemente sind
   `{id, element_type, position: [x,y], properties: {name: value}}`;
   `properties`-Werte sind rohe Scalare (String/Zahl/Bool), kein Wrapper-Typ.
3. `POST /api/flows/{id}/start` — baut die GStreamer-Pipeline aus
   Elementen/Blocks/Links und setzt sie auf `Playing`. Antwort enthält
   `running: true`, `gst_state: "Playing"`.
4. `POST /api/flows/{id}/stop` — Pipeline wird sauber abgebaut
   (`running: false`, `gst_state: null`).
5. `DELETE /api/flows/{id}` — entfernt den Flow aus dem Storage.

`GET /api/flows/{id}` liefert jederzeit den aktuellen Zustand — das ist die
Grundlage für State-Diffing im Client (siehe WS/Events unten).

## Block-Registry-Mechanik

Ein **Block** ist eine höherwertige, wiederverwendbare Einheit (z. B. "NDI
Input", "Vision Mixer") mit einer deklarativen Property-Fassade vor den
eigentlichen GStreamer-Elementen. Drei Teile, immer zusammen:

**1. Metadata** (`BlockDefinition`) — für Palette/Inspector im Client. Beispiel
(gekürzt aus `backend/src/blocks/builtin/ndi.rs`):

```rust
fn ndi_input_definition() -> BlockDefinition {
    BlockDefinition {
        id: "builtin.ndi_input".to_string(),
        name: "NDI Input".to_string(),
        category: "Inputs".to_string(),
        exposed_properties: vec![
            ExposedProperty {
                name: "ndi_name".to_string(),
                property_type: PropertyType::String,
                mapping: PropertyMapping {
                    element_id: "_block".to_string(),   // an den Block selbst, nicht an ein Sub-Element
                    property_name: "ndi_name".to_string(),
                    transform: None,                     // optionale Wertumrechnung
                },
                live: false,   // während Playing änderbar?
                ..Default::default()
            },
            // ...
        ],
        ..Default::default()
    }
}

pub fn get_blocks() -> Vec<BlockDefinition> {
    if !is_ndi_available() {           // Laufzeit-Check: passendes GStreamer-Plugin da?
        return vec![];                  // -> Block taucht gar nicht in der Palette auf
    }
    vec![ndi_input_definition(), ndi_output_definition()]
}
```

**2. Builder** (`impl BlockBuilder`) — baut zur Laufzeit die echten
GStreamer-Elemente aus den Property-Werten einer Instanz:

```rust
pub struct NDIInputBuilder;

impl BlockBuilder for NDIInputBuilder {
    fn build(&self, instance_id: &str, properties: &HashMap<String, PropertyValue>,
              ctx: &BlockBuildContext) -> Result<BlockBuildResult, BlockBuildError> {
        // gst::ElementFactory::make("ndisrc")..., mit instance_id-Namespacing
    }
}
```

**3. Dispatch** — `builtin::get_builder(block_definition_id: &str)` (in
`backend/src/blocks/builtin/mod.rs`) matcht die `id` aus Schritt 1 auf eine
Builder-Instanz aus Schritt 2:

```rust
pub fn get_builder(block_definition_id: &str) -> Option<Arc<dyn BlockBuilder>> {
    match block_definition_id {
        "builtin.ndi_input" => Some(Arc::new(ndi::NDIInputBuilder)),
        // ...
        _ => None,
    }
}
```

`builtin::get_all_builtin_blocks()` sammelt alle `get_blocks()`-Aufrufe zu
einer Liste ein; `BlockRegistry` (`backend/src/blocks/registry.rs`) kombiniert
das mit nutzerdefinierten Blocks (JSON-persistiert, `user.<uuid>`-Präfix,
können nicht die `builtin.*`-IDs überschreiben oder löschen).

**Für unser OMT-Vorhaben (M2, Option a)**: ein `OmtBlock` würde exakt diesem
Dreiklang folgen — `get_blocks()` mit einem Laufzeit-Verfügbarkeitscheck
(analog `is_ndi_available()`), ein `BlockBuilder`, der gegen die libomt-C-FFI
baut, und ein neuer Match-Arm in `get_builder()`.

## Vision-Mixer (PVW/PGM)

Ein Block-basierter Baustein (`backend/src/blocks/builtin/vision_mixer/`),
kein eigener Flow-Typ. Broadcast-Style PGM/PVW-Switcher: Programm- und
Preview-Bus, Take/Cut, Dip/Wipe-Übergänge über bestehende Transitions, PiP,
DSK, GPU-Shader-FX (GLSL-Looks). GPU-Compositing über OpenGL; fällt bei reinem
Software-GL (Mesa llvmpipe) automatisch auf einen CPU-Compositor zurück, weil
llvmpipe langsamer ist als CPU-Compositing (siehe
`workspace/strom/docs/HARDWARE_REQUIREMENTS.md`). Bedienungsdetails:
`workspace/strom/docs/VISION_MIXER_OPERATOR_GUIDE.md`.

## Audio-Mixer

Ebenfalls Block-basiert. Digitale Mischkonsole: Kanäle, Aux-Sends, Gruppen,
Metering, Loudness. Details:
`workspace/strom/docs/AUDIO_MIXER_OPERATOR_GUIDE.md`.

## REST- und WebSocket-API

- Interaktive OpenAPI-Doku: `/swagger-ui`, Schema-Datei `openapi.json` im
  strom-Repo-Root (Snapshot-Test: `cargo test --test openapi_test`).
- `WS /api/ws`: Verbindungsaufbau liefert sofort eine Bestätigungsnachricht
  (verifiziert: "Received text message from backend - connection confirmed"),
  danach laufende `StromEvent`-Diffs (z. B. `FlowsLoaded`, `ElementsLoaded`,
  `BlocksLoaded`) statt vollständiger Re-Fetches — das ist das State-Diffing,
  auf dem der native Client seinen lokalen Cache hält.
- `GET /api/events`: SSE-Alternative zum selben Event-Strom.
- Jeder Endpunkt braucht laut `workspace/strom/CLAUDE.md` eine
  `#[utoipa::path(...)]`-Annotation UND eine Registrierung in `openapi.rs` —
  beides zusammen, sonst taucht er nicht im Schema auf.
- Model Context Protocol (MCP) läuft über `/api/mcp` als Modul im Backend
  (`backend/src/mcp/`) — **kein eigenes Crate** trotz gegenteiliger Annahme in
  `claude.md` (siehe `docs/DECISIONS.md`).

## Storage-Modell

`Config` (`backend/src/config.rs`) wählt zur Laufzeit zwischen:

- **JSON-Dateien** (Default): `flows.json`/`blocks.json` unter
  `~/.local/share/strom/` (Linux-Default, überschreibbar per `--data-dir`
  bzw. `--flows-path`/`--blocks-path`).
- **PostgreSQL**: aktiviert durch `--database-url`/`STROM_DATABASE_URL`.
  Details: `workspace/strom/docs/POSTGRESQL.md`.

Konfigurationspriorität (höchste zuerst): CLI-Argumente > Env-Variablen
(`STROM_*`) > lokale `.strom.toml` > User-Config (`~/.config/strom/config.toml`)
> Defaults. Implementiert über `figment` (`Config::from_figment`).

## Gotcha: `PropertyValue` ist `#[serde(untagged)]`

`strom_types::element::PropertyValue` (`String | Int(i64) | UInt(u64) | Float(f64)
| Bool`) trägt `#[serde(untagged)]`, `Int` vor `UInt` deklariert. Ein
schlichtes JSON-Integer-Literal (`"num_channels": 2`) deserialisiert deshalb
immer als `Int`, nie als `UInt` — unabhängig davon, welche Variante die
Property "eigentlich" meint. Code, der auf `PropertyValue::UInt` matcht, um
einen aus JSON gelesenen Zähler zu erkennen, matcht in der Praxis nie.
Betroffen: alles was Property-Werte aus einem `Flow`/`BlockInstance` (nicht
aus einer Live-`GET .../properties`-Antwort, die eigene Regeln hat, siehe
unten) liest. Workaround: entweder beide Varianten matchen, oder — robuster —
den Wert aus einer Live-Quelle ableiten statt aus dem gespeicherten JSON zu
raten (siehe `workspace/app-desktop/src/main.rs`, `num_channels`-Herleitung
aus den tatsächlich zurückgegebenen `ch{N}_fader`-Keys).

## Property-System

`ExposedProperty` bildet eine Block-Property (z. B. `ndi_name`) auf ein
konkretes GStreamer-Element und dessen Property ab (`PropertyMapping {
element_id, property_name, transform }`). `element_id: "_block"` heißt: die
Property wirkt direkt auf den Block selbst (nicht auf ein benanntes
Sub-Element der Pipeline). `live: bool` markiert, ob die Property auch
während `Playing` geändert werden darf; `transform` erlaubt eine
Werteumrechnung zwischen UI-Repräsentation und GStreamer-Property. Dieselbe
Fassade gilt für rohe `elements[].properties` in einem Flow — dort ohne
Mapping-Indirektion, direkt als `{property_name: value}`.

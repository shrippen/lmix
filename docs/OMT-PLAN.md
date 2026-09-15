# OMT-Plan (M2-Vorbereitung)

Recherche-Stand, verifiziert gegen die echten GitHub-Repos (nicht nur aus
`claude.md` übernommen). Reine Doku/Planung — keine Implementierung in M0.

## libomt: Quelle, Lizenz, Build

- Repo: `openmediatransport/libomt` — "A C wrapper for the libomnet [sic,
  gemeint: libomtnet] library", MIT-lizenziert.
- Kompanion-Repo: `openmediatransport/libomtnet` (.NET-Standard-2.0-
  Referenzimplementierung des Protokolls), ebenfalls MIT.
- **Mechanismus**: libomt nutzt .NET Native AOT, um `libomtnet` in eine
  native Shared Library mit C-Exports (`libomt.h`) zu übersetzen — daher der
  .NET-8-SDK-Bedarf beim Kompilieren, obwohl das Ergebnis eine reine
  C-Bibliothek ohne .NET-Laufzeitabhängigkeit ist.
- **Kein Linux-Prebuilt**: laut `openmediatransport/.github/DOWNLOADS.md`
  gibt es offizielle Binärpakete nur für Windows und macOS. Auf Linux muss
  aus Quelle gebaut werden — deckt sich mit der Annahme in `claude.md`.
- Build (bestätigt aus `libomt/README.md` + `libomt/build/`):
  1. Verzeichnisstruktur `[root]/libomtnet` und `[root]/libomt` nebeneinander
     anlegen (Build referenziert `../libomtnet` relativ).
  2. .NET 8 SDK installieren (nur zum Kompilieren, nicht zur Laufzeit nötig).
  3. `libomt/build/buildlinuxx64.sh` ausführen (Pendant für ARM64:
     `buildlinuxarm64.sh`, existiert ebenfalls).
  4. Ergebnis: Shared Library im plattform-nativen Zielverzeichnis (analog
     Windows: `bin/Release/net8.0/win-x64/native`).
  5. **Offener Schritt, noch nicht ausgeführt**: `ldd` gegen die gebaute
     `.so` laufen lassen, um zu verifizieren, dass keine .NET-Runtime-
     Abhängigkeiten (`libcoreclr.so` o. ä.) übrig bleiben — AOT-Kompilate
     sollten davon frei sein, das aber auf unserem Host nachweisen statt
     anzunehmen. Dafür ist die Installation des .NET 8 SDK nötig (invasiv,
     bislang nicht vorgenommen — bewusst zurückgestellt, bis wir konkret mit
     M2 anfangen).

## Rust-Anbindung: zwei community Optionen, nur eine zulässig

Bei der Recherche gefunden, in `claude.md` nicht erwähnt:

| Projekt | Ansatz | Lizenz | Zulässig? |
|---|---|---|---|
| `udoschneider/omt-rs` | Echte FFI-Bindings gegen `libomt.h` (`omt-sys` = raw FFI, `omt` = sicherer Wrapper) | MIT OR Apache-2.0 | **Ja** — geht durch libomt |
| `MikanseiLaboratory/openmediatransport-rs` | "Pure Rust implementation" des Protokolls, eigener VMX1-Codec | MIT | **Nein** — verletzt den harten Constraint "OMT-Protokoll wird nicht neu implementiert" |

`omt-rs` ist noch nicht auf crates.io (`publish = false`, API "settles noch"),
müsste also als Git-Dependency eingebunden werden. Das reduziert den Aufwand
für Option (a) deutlich: keine eigene C-FFI-Schicht gegen `libomt.h` nötig,
nur ein `build.rs`/Linker-Setup, das gegen unsere selbst gebaute `libomt.so`
linkt, plus die eigentliche Block-Integration.

## Option (a): OmtBlock in der strom-Block-Registry

Folgt exakt dem in `docs/STROM-ANATOMY.md#block-registry-mechanik`
beschriebenen Dreiklang (Vorbild: `backend/src/blocks/builtin/ndi.rs`):

1. `get_blocks()` mit Laufzeit-Verfügbarkeitscheck (z. B.: kann
   `libomt.so` geladen werden? — analog `is_ndi_available()`).
2. `OmtInputBuilder: BlockBuilder`, baut aus `omt-rs`
   (`Receiver`/`Discovery`) heraus einen GStreamer-`appsrc` und pumpt
   empfangene Frames hinein (kein natives GStreamer-OMT-Element vorhanden,
   also Bridge über `appsrc`/`appsink`, wie es strom auch für andere
   Nicht-GStreamer-native Quellen macht).
3. Match-Arm in `builtin::get_builder()`.

**Aufwand-Treiber**: Frame-Marshalling zwischen `omt-rs`-Buffern und
GStreamer-Buffers (Pixelformat-Konvertierung, Timing/PTS-Mapping), Threading
(OMT-Receive-Loop darf den GStreamer-Streaming-Thread nicht blockieren —
SPSC-Queue mit Frame-Drop bei Backpressure, wie in `claude.md` für Option (b)
skizziert, gilt hier genauso). GPU-Pfad (`omt-rs`' `wgpu`-Feature) ist
optional, für M2 erstmal nicht nötig (CPU-Pfad reicht für den
Latenzbudget-Test).

**Grobschätzung**: 3-5 Tage für einen funktionsfähigen Input-Block inkl.
Discovery-UI-Anbindung, vorausgesetzt `omt-rs` funktioniert wie dokumentiert
gegen unsere selbst gebaute `libomt.so`.

## Option (b): omtpw-bridge (externes Adapter-Repo)

OMT-Receive-Loop (über `omt-rs`, gleiche Bibliothek wie Option (a)) → NDI-Send
oder SRT-Video-Out → bestehender NDI-/SRT-Block in strom, unverändert.

**Vorteil**: kein Rust-Code in strom selbst, strom bleibt komplett unberührt
(minimalster Diff überhaupt). **Nachteil**: zusätzlicher Prozess, zusätzlicher
Encode/Decode-Hop (OMT→NDI oder OMT→SRT, je nach gewähltem Ausgang), damit
zusätzliche Latenz — genau das Latenzbudget (< 5 Frames OMT-Rx → PGM-Ausgabe),
das M2 als Akzeptanzkriterium nennt, ist mit einem Zwischen-Prozess deutlich
schwerer zu halten als mit direktem `appsrc`-Einspeisen in Option (a).

**Grobschätzung**: 2-3 Tage für die Bridge selbst (dünner als (a), da kein
GStreamer-Block-Wiring), aber vermutlich zusätzlicher Latenz-Tuning-Aufwand,
der schwer vorab zu schätzen ist.

## Empfehlung (zur Prüfung in M2, nicht jetzt entschieden)

Tendenz zu **Option (a)**: `omt-rs` senkt den ursprünglich für (a)
angenommenen Mehraufwand (eigene C-FFI) so weit, dass der Latenzvorteil eines
direkten Blocks den Mehraufwand gegenüber (b) vermutlich aufwiegt. Endgültige
Entscheidung erst nach echtem libomt-Build + kurzem `omt-rs`-Spike (Empfang
eines Test-Streams gegen `omtsendtest`), wie in `claude.md` für M2 vorgesehen.

## Nächste konkrete Schritte (M2, nicht M0)

1. ~~.NET 8 SDK installieren, `libomt` + `libomtnet` nebeneinander klonen,
   `buildlinuxx64.sh` laufen lassen, `ldd` gegen das Ergebnis verifizieren.~~
   **Erledigt** — siehe `docs/DECISIONS.md`, Eintrag 2026-09-15 "libomt-Build
   verifiziert + omt-rs-Spike erfolgreich".
2. ~~`omt-rs` als Git-Dependency in einem Wegwerf-Testprojekt gegen die
   selbst gebaute `libomt.so` linken, `Discovery::get_addresses()` prüfen.~~
   **Erledigt**, siehe selber Eintrag. (Ohne laufenden `omtsendtest` als
   Gegenstelle — bestätigt wurde der FFI-Link + ein echter mDNS-Scan, kein
   tatsächlicher Frame-Empfang. Das ist der nächste Schritt.)
3. ~~Aufwandsschätzung oben verifizieren/korrigieren, dann Options-Entscheidung
   (a) vs (b) treffen~~ — **Entscheidung getroffen: Option (a)**, siehe
   `docs/DECISIONS.md`.
4. Jetzt offen: `omtsendtest` (oder ein anderer echter OMT-Sender) als
   Gegenstelle aufsetzen, echten Frame-Empfang über `Receiver` verifizieren,
   dann `OmtBlock` in `backend/src/blocks/builtin/` bauen (Vorbild:
   `ndi.rs`), inkl. `appsrc`-Bridge und SPSC-Queue mit Frame-Drop.

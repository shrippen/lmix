Title: Device discovery: list DeckLink cards as video sources

## What is missing

DeckLink devices announce the class `Video/Source/Hardware`. The device monitor's `Video/Source` filter lets them in, but the category is matched against the class exactly, so they land in "other" and no client finds them as video sources. A client also needs the card's device number to build a DeckLink Input block; the provider gives it as an integer, which the string-only property copy drops.

The cards' sound is a second trap: `decklinkaudiosrc` captures only next to the `decklinkvideosrc` of the same device, so offering it as an audio source for a Local Input gives an input that never starts.

## Proposed direction

- Categorise by the first two parts of the class (`Video/Source`), so hardware video sources are found.
- Keep integer device properties as text, with `device-number` and `persistent-id` in the kept list; the persistent id is the stable key. Name the provider (`decklinkdeviceprovider`, label "DeckLink").
- Keep DeckLink sound out of the audio sources; it comes with the DeckLink Input block.

## Questions for the maintainers

1. Matching on two class parts may let other `.../Hardware` devices into the video sources too. Wanted, or only for DeckLink?
2. Should a discovered DeckLink card point at the DeckLink Input block directly (a suggested block type in the device response)?

## Test case

Unit tests on the conversion with a fake `Video/Source/Hardware` device carrying `device-number` (int) and `persistent-id`: it lands in the video category with both properties as text; a fake DeckLink `Audio/Source` lands in "other". Needs no card.

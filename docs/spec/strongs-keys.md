# Spec: dictionary key normalisation

**Applies to:** looking up keys in dictionary modules (`RawLD`, `RawLD4`, `zLD`).

**Source of this spec:** observed behaviour of libsword 1.9.0: keys were
written to a `RawLD` module and the stored keys read back with `mod2imp`.

## Strong's padding (on unless the conf has `StrongsPadding=false`)

Applied to keys whose length is 1–8 bytes. Otherwise the key is unchanged.

1. An optional first character `G`, `g`, `H` or `h` is a *prefix*.
2. After the prefix, take the longest run of ASCII digits, *D*. Let *R* be the
   remainder of the key after *D*.
3. If *D* is empty, or *R* is longer than 2 bytes, the key is unchanged.
4. Otherwise the result is: the prefix (as given), then the numeric value of *D*
   zero-padded to **4** digits if there is a prefix or **5** digits if not (never
   truncated), then a suffix taken from *R*:
   * *R* starts with an ASCII letter → that letter, upper-cased;
   * *R* is `!` followed by an ASCII letter → `!` and the letter, upper-cased;
   * anything else → no suffix (the rest of *R* is dropped).

## Case folding (on unless the conf has `CaseSensitiveKeys=true`)

Keys being looked up, and keys read from the index, are compared after
upper-casing (Unicode upper-casing).

## Observed examples

| key | stored as |
|---|---|
| `25` | `00025` |
| `G25` | `G0025` |
| `h430a` | `H0430A` (upper-cased by case folding) |
| `3056!b` | `03056!B` |
| `123.` | `00123` |
| `25ab` | `00025A` |
| `12a3` | `00012A` |
| `1-2` | `00001` |
| `1!` | `00001` |
| `H0` | `H0000` |
| `12345678` | `12345678` |
| `123456789` | `123456789` (9 bytes: unchanged) |
| `+5`, `G`, `AARON` | unchanged |

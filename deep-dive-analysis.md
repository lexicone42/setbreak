# SetBreak Deep Dive: What 10,000 Live Jam Tracks Tell Us

*Analysis of 5,761 Grateful Dead + 3,201 Phish + 1,182 Built to Spill live tracks using 10 algorithmic jam scores derived from DSP audio analysis.*

## The Scoring System

Each track is scored 0-100 on 10 dimensions computed from raw audio features (spectral analysis, onset detection, chromagram, loudness dynamics, chord detection, etc.):

| Score | What it measures |
|-------|-----------------|
| **Energy** | Raw power — RMS, LUFS, bass, spectral centroid |
| **Intensity** | Dynamic extremes — flux variance, dynamic range, loudness range |
| **Groove** | Rhythmic regularity — onset rate, flux consistency, bass steadiness |
| **Improvisation** | Temporal development — duration, energy peaks, dynamics peaks, tempo instability |
| **Tightness** | Ensemble precision — flux CV, tempo stability, zero-crossing consistency |
| **Build Quality** | Energy arc detection — does the track build to climaxes? |
| **Exploratory** | Harmonic breadth — chord vocabulary, chromagram entropy, rhythmic unpredictability |
| **Transcendence** | Peak experience composite — build quality + groove×energy synergy + sustained power |
| **Valence** | Brightness/positivity — spectral brightness, inverse roughness, major chord ratio |
| **Arousal** | Activation level — energy + roughness + tempo + flux + loudness |

---

## 1. The Fundamental Tension: Groove vs. Improvisation

Only **3 tracks out of 10,000** score above 50 on both groove AND improvisation. All three are early Dead (1968-72).

These two dimensions are structurally opposed (correlation r = -0.24). Groove rewards rhythmic regularity and ensemble lock; improvisation rewards temporal unpredictability and departure from structure. A band can't easily do both at the same time.

The full 10×10 correlation matrix reveals two independent axes of jam music:

- **The Jam Axis**: improvisation, exploratory, transcendence, build quality (all correlated r > 0.36)
- **The Groove Axis**: groove, tightness (correlated r = 0.61)

Energy anti-correlates with almost everything interesting (r = -0.50 with intensity, -0.51 with valence). Loud doesn't mean good.

### Songs That Balance Both

The songs that come closest to balancing groove and improvisation (gap < 5 pts, both > 30):

| Song | Band | Groove | Improvisation | Transcendence |
|------|------|--------|---------------|---------------|
| Down With Disease | Phish | 39.3 | 43.0 | 60.2 |
| Bird Song | Dead | 40.2 | 39.6 | 64.2 |
| Tweezer | Phish | 38.8 | 38.0 | 58.8 |
| Mercury | Phish | 37.2 | 35.4 | 61.2 |
| Bathtub Gin | Phish | 37.2 | 35.0 | 58.1 |

These are songs built on a groove that then departs from it — the jam starts from a rhythmic foundation and wanders. A different mode than Dark Star (which starts unstructured) or a tight funk jam (which never leaves the groove).

---

## 2. The Grateful Dead's Evolution: A Story in Three Acts

Year-by-year averages across all live tracks:

| Year | n | Imp | Exp | Groove | Tightness | Transcend | Build | Valence | Arousal |
|------|---|-----|-----|--------|-----------|-----------|-------|---------|---------|
| 1966 | 33 | 9.5 | 26.8 | **48.2** | 47.0 | 30.9 | 21.4 | 37.0 | 30.1 |
| 1968 | 355 | 17.3 | 27.7 | 39.4 | 48.6 | 38.9 | 36.9 | 34.4 | 41.6 |
| **1969** | 96 | **23.9** | 32.8 | 39.3 | 51.4 | 44.9 | **45.1** | 35.7 | 35.6 |
| 1970 | 676 | 15.4 | 32.2 | 38.8 | 48.8 | 39.0 | 37.7 | 36.4 | 36.7 |
| 1971 | 1882 | 15.2 | 34.1 | 41.8 | **55.8** | 42.2 | 34.1 | 32.4 | 37.8 |
| **1972** | 182 | 18.6 | **38.3** | 39.5 | 52.6 | 45.2 | 38.3 | 33.0 | 38.2 |
| **1973** | 51 | 17.1 | **38.6** | 38.5 | 55.0 | **47.9** | **42.9** | **29.3** | **45.2** |
| 1974 | 265 | 18.5 | 37.7 | 40.1 | 54.6 | 44.1 | 39.5 | 34.1 | 36.6 |
| **1975** | 320 | **9.2** | **20.2** | 37.8 | 50.1 | **25.1** | 34.6 | 36.5 | 33.1 |
| 1976 | 70 | 20.2 | 38.2 | **29.9** | 55.8 | 47.5 | 38.2 | 30.2 | 39.5 |
| 1977 | 148 | 21.6 | 37.6 | 38.3 | 55.6 | 44.6 | 41.6 | 33.1 | 35.7 |
| **1978** | 114 | 22.1 | **41.9** | 41.0 | **57.5** | 48.1 | 43.0 | 36.8 | 37.0 |
| 1979 | 150 | 19.0 | 35.5 | 39.4 | **58.7** | 44.8 | 39.2 | 31.0 | 40.1 |
| **1981** | 69 | 19.7 | **44.0** | 42.4 | **59.2** | **50.4** | **46.0** | 37.3 | 35.4 |
| 1985 | 124 | 17.0 | 40.5 | **43.7** | 58.8 | 46.7 | 40.2 | 37.5 | 35.2 |
| **1988** | 105 | 20.5 | **43.4** | **35.0** | 55.2 | 48.2 | 40.1 | 35.5 | 40.9 |
| **1991** | 100 | **23.0** | 41.6 | 40.7 | 57.3 | **52.0** | **43.6** | 36.0 | 35.1 |
| 1993 | 82 | 21.3 | 41.0 | **42.8** | 57.0 | 48.3 | 43.7 | 37.5 | 30.8 |

### The Story

**Act I: Discovery (1966-1969)** — The Dead start as a garage band (1966: groove 48.2, improvisation 9.5). By 1969 they've discovered temporal risk-taking — improvisation hits 23.9 (the highest until 1991) and build quality peaks at 45.1. This is the primal era: raw, dangerous, committed.

**Act II: Expansion and Collapse (1970-1977)** — The psychedelic period (70-72) brings harmonic breadth (exploratory rises to 38.3) while improvisation dips. 1973 is the *darkest year* in the data — lowest valence (29.3) and highest arousal (45.2), but also the most transcendent pre-1981 (47.9). These are brooding, intense performances. Then 1975 — the hiatus — is a catastrophic collapse across every dimension. Improvisation hits 9.2, transcendence 25.1. The lowest point in the band's history.

**Act III: Reinvention (1978-1995)** — The Dead rebuild as a fundamentally different band. 1978-79 is the tightest Dead ever (tightness 57.5-58.7). By 1981, they hit a remarkable peak: highest exploratory (44.0), highest transcendence (50.4), and highest tightness (59.2) *simultaneously*. The MIDI years (1986-90) push harmonic breadth to its peak (43.4 in 1988) but sacrifice groove (35.0 — tied for the least groovy Dead). The final years (1991-95) are a genuine late-career renaissance: improvisation reaches 23.0 (matching 1969), transcendence hits 52.0 (the all-time peak), and build quality reaches 43.6. The old lions could still roar.

### 1973: The Darkest Year

The data confirms what Deadheads intuit about this period. With a valence of 29.3 (the lowest of any year) and arousal of 45.2 (the highest), this is the most emotionally intense Dead era. The Wake of the Flood / early Mars Hotel period was *brooding* — dark, driving, harmonically rich. The library captures two shows from this year (3/16/73 and 11/17/73) and both are dominated by tracks scoring in the high 60s-70s for transcendence despite relatively modest improvisation scores. These aren't long free-form explorations — they're tightly wound, emotionally heavy performances.

---

## 3. Phish Year-by-Year: 1997 Is Real, But 2015 Might Be Better

| Year | n | Imp | Exp | Groove | Tightness | Transcend | Build |
|------|---|-----|-----|--------|-----------|-----------|-------|
| 1989 | 23 | 16.3 | 29.9 | **45.3** | 54.1 | 40.9 | 34.8 |
| **1994** | 40 | 14.9 | 28.1 | **46.3** | **55.6** | 45.8 | 37.6 |
| 1995 | 32 | 18.2 | **38.3** | 36.3 | 54.3 | 41.1 | 31.7 |
| **1997** | 112 | **23.7** | **36.6** | **42.0** | **56.7** | **50.8** | **44.4** |
| 1998 | 35 | 21.8 | 34.2 | **43.9** | **57.3** | **51.1** | 42.4 |
| 1999 | 17 | 21.7 | **40.2** | 39.1 | 51.7 | 50.7 | 39.9 |
| 2000 | 16 | 23.7 | 25.6 | **44.8** | 53.3 | 48.5 | 43.3 |
| **2003** | 25 | **24.9** | 32.5 | 33.7 | 48.5 | 42.8 | 35.6 |
| **2015** | 567 | 22.1 | **40.4** | 38.5 | 50.8 | **52.3** | 41.7 |
| 2018 | 164 | **22.7** | **40.3** | 35.9 | 51.6 | 50.9 | 42.0 |

### The Narrative

**1994 is the grooviest Phish** (46.3) and the tightest early Phish (55.6). Pure rhythmic lock.

**1997 is the balanced peak** — highest in improvisation (23.7), groove (42.0 for a "peak" year), tightness (56.7), build quality (44.4). Everything fires simultaneously.

**2003 Reunion is the most improvised Phish** (24.9) but the least transcendent (42.8) and least groovy (33.7). Maximum risk-taking without the payoff — a band relearning each other.

**2015 is the surprise**: it *beats* 1997 on transcendence (52.3 vs 50.8) and exploration (40.4 vs 36.6). 3.0 Phish is genuinely competitive with the legendary era by these metrics. They're less tight and less groovy, but they find peaks more often.

**2018+ holds**: 4.0 Phish maintains the high standards of 3.0.

---

## 4. Dead vs. Phish: Architecturally Different Bands

| Metric | Dead | Phish |
|--------|------|-------|
| Avg Improvisation | 17.0 | **20.0** |
| Avg Exploratory | 34.9 | **35.3** |
| Avg Groove | **40.2** | 37.3 |
| Avg Transcendence | 42.5 | **48.9** |
| % tracks >60 transcendence (5min+) | 32.3% | **36.0%** |
| % tracks >50 improvised (5min+) | 5.0% | **6.3%** |
| % tracks >50 exploratory (5min+) | **19.6%** | 7.0% |

**Phish is more transcendent**. Their average transcendence is 6.4 points higher, and a larger percentage of their extended tracks achieve peak states. This likely reflects Phish's composed climax structures — songs like Divided Sky, Slave, and Harry Hood have built-in peak architecture that the Dead's more open-ended forms don't guarantee.

**The Dead are massively more exploratory**. Nearly 20% of Dead tracks >5 min score above 50 for exploration, vs only 7% for Phish. Dead Space jams, feedback explorations, and atonal passages cover harmonic territory that has no Phish equivalent.

**The Dead are groovier**. Counter-intuitive, but the data says the Dead maintain more rhythmic consistency on average. Phish's funk jams are grooviest-of-the-groovy, but the average Phish track is less rhythmically locked than the average Dead track.

---

## 5. Jam Vehicle Personality Profiles

### Phish: Most Reliable Transcenders

Songs with 8+ performances over 8 minutes, ranked by % that hit transcendence > 60:

| Song | Performances | % Transcendent | Avg Trn | Range |
|------|-------------|----------------|---------|-------|
| **Divided Sky** | 14 | **100%** | 66.0 | 62-72 |
| **Winterqueen** | 9 | **100%** | 63.3 | 61-66 |
| Slave to the Traffic Light | 22 | 95% | 65.0 | 58-70 |
| Harry Hood | 32 | 91% | 64.7 | 53-73 |
| You Enjoy Myself | 21 | 90% | 63.3 | 58-72 |
| Walls of the Cave | 18 | 89% | 62.1 | 59-65 |

**Divided Sky** hits transcendence in *every single performance* in the library. Its composed climax structure guarantees a peak. **Winterqueen** is the most *consistent* song overall — variance of just 2.5 (vs 33.1 for Blaze On).

### Phish: Biggest Wild Cards

Songs with highest transcendence variance (most unpredictable):

| Song | Performances | Avg Trn | Variance | Range |
|------|-------------|---------|----------|-------|
| **Blaze On** | 18 | 51.2 | 33.1 | 44-64 |
| **Sand** | 24 | 55.5 | 31.8 | — |
| **Simple** | 14 | 56.4 | 30.9 | — |
| **2001** | 9 | 61.4 | 27.9 | — |
| **46 Days** | 15 | 57.3 | 26.7 | — |

**Blaze On** is the most unpredictable jam vehicle in the library. Best Blaze On (8/9/15, 8.1 min): trn=64. Worst (6/29/16, 8.3 min): trn=44. Nearly identical duration, completely different outcome. The song is a blank canvas.

### Phish: Most Variable by Improvisation Range

| Song | n | Avg Imp | Imp Range | Character |
|------|---|---------|-----------|-----------|
| **Fuego** | 40 | 31.2 | **62.7** | 5 to 68 — from throwaway to epic |
| **Chalk Dust Torture** | 37 | 30.4 | **61.1** | The original stretch vehicle |
| **AC/DC Bag** | 12 | 15.6 | **58.0** | Usually compact, occasionally explodes |
| **Down With Disease** | 26 | 43.0 | 51.1 | Consistently extended |

**Fuego's range is staggering**: imp scores from 5 (8/23/15, 7.9 min) to 68 (7/8/14, 26 min). Same song, wildly different commitment to exploration.

### Dead: Jam Vehicles by Transcendence

| Song | n | Avg Imp | Avg Exp | Avg Trn | Avg Build | Avg Min |
|------|---|---------|---------|---------|-----------|---------|
| **Dark Star ->** | 6 | **70.0** | 48.7 | **72.4** | **75.0** | 24.7 |
| Morning Dew | 6 | 28.8 | 45.5 | **70.3** | 64.2 | 10.9 |
| Bird Song | 5 | 39.6 | **53.1** | 64.2 | 55.4 | 13.7 |
| Playing in the Band | 5 | 47.0 | 44.6 | 62.2 | 54.2 | 13.3 |
| Sugaree | 6 | 41.0 | **53.9** | 60.1 | 53.9 | 12.3 |
| Space -> | 10 | 34.8 | **55.4** | 57.8 | 44.0 | 8.5 |

**Dark Star** dominates improvisation (70.0) AND transcendence (72.4) AND build quality (75.0). It's the single most reliable vehicle for peak musical experience in the Dead's repertoire.

**Morning Dew** achieves transcendence (70.3) in just 10.9 minutes with relatively low improvisation (28.8). It's transcendence through emotional arc, not exploration — the song's structure itself builds to a devastating climax.

**Space** is the most exploratory vehicle (55.4) — harmonic territory that nothing else covers — but its transcendence lags (57.8) because it often lacks the build/groove foundation.

---

## 6. Song Personalities: Improvisatory vs. Exploratory

With the newly separated scores (r=0.60, down from r=0.88), we can distinguish songs that lean toward temporal development vs. harmonic breadth:

### Dead: Improvisation-Leaning Songs (imp > exp)
| Song | Avg Imp | Avg Exp | Gap |
|------|---------|---------|-----|
| Drums -> | 36.2 | 31.9 | +4.4 |

### Dead: Exploration-Leaning Songs (exp > imp)
| Song | Avg Imp | Avg Exp | Gap |
|------|---------|---------|-----|
| Big River | 12.6 | 42.3 | **-29.7** |
| Sugar Magnolia | 21.7 | 51.0 | **-29.3** |
| I Know You Rider | 13.7 | 41.0 | -27.3 |
| Loser | 20.5 | 45.6 | -25.1 |
| Space -> | 37.7 | 57.8 | -20.1 |

Almost every Dead song leans exploratory. The Dead's default mode is harmonic breadth — covering lots of tonal territory even within structured songs. Only Drums -> leans improvisatory, because it's literally a percussion solo — all temporal development, no harmonic content.

---

## 7. Short Transcendence: The 6-Minute Miracle

Tracks under 7 minutes that achieve transcendence > 65:

| Track | Date | Band | Min | Trn | Build | Imp | Character |
|-------|------|------|-----|-----|-------|-----|-----------|
| **Wingsuit** | 10/31/13 | Phish | 6.3 | **79** | 74 | 18 | Halloween peak |
| Sleeping Monkey | 12/30/97 | Phish | 6.2 | 71 | 51 | 15 | NYE energy |
| Stella Blue -> | 6/30/86 | Dead | 7.6 | 72 | 60 | 21 | Emotional peak |
| 2001 | 8/28/16 | Phish | 6.3 | 69 | 51 | — | Pure power |
| Fast Enough For You | 6/30/19 | Phish | 6.0 | 69 | 53 | — | Ballad peak |

**Wingsuit 10/31/13** is remarkable: transcendence of 79 (higher than most 25-minute Dark Stars) in 6.3 minutes, with a build quality of 74 but improvisation of only 18. This is transcendence through *composed intensity* — the song's structure itself creates the peak experience, not improvisation.

This reveals two distinct paths to transcendence:
1. **Through improvisation**: 25-minute Dark Stars, building and releasing over extended temporal arcs
2. **Through composition**: 6-minute Wingsuits, with architecturally perfect emotional curves

Both register as peak experience in the audio features. The algorithm doesn't know the difference — it just sees sustained energy, dynamic arc, and harmonic richness hitting simultaneously.

---

## 8. The Valence Landscape

### The Darkest Year: Dead 1973

| Year | Valence | Arousal | Character |
|------|---------|---------|-----------|
| 1973 | **29.3** | **45.2** | Dark, driving, harmonically intense |
| 1976 | 30.2 | 39.5 | Post-hiatus blues |
| 1979 | 31.0 | 40.1 | End of an era |
| 1971 | 32.4 | 37.8 | Psychedelic murk |

1973 stands alone as both the darkest AND most aroused Dead year. Low valence + high arousal = *aggression*. This maps to the Wake of the Flood period — music that's emotionally heavy, driving, and intense without being "happy."

### The Imp-Exp Gap Over Time

Every single year of the Dead's career, exploratory exceeds improvisation (gap of -8.9 to -24.2 pts). The Dead are constitutionally an exploratory band — they cover harmonic territory as a default behavior, whether they're improvising or not. Even their tightest, most structured performances (1994 tightness 55.6) are more harmonically broad than they are temporally adventurous.

---

## 9. The Canonical Jam: Veneta Dark Star (8/27/72)

Percentile ranks for the single most famous jam in the library:

| Score | Value | Percentile | Rank |
|-------|-------|-----------|------|
| Improvisation | 69 | **99.4th** | 59/9985 |
| Build Quality | 69 | **98.3rd** | 167/9985 |
| Transcendence | 65 | **92.0th** | 794/9985 |
| Groove | 51 | 86.2nd | 1381/9985 |
| Exploratory | 47 | 82.1st | 1792/9985 |
| Valence | 39 | 72.3rd | 2761/9985 |
| Intensity | 36 | 45.5th | 5445/9985 |
| Arousal | 31 | 17.5th | 8242/9985 |
| Tightness | 49 | 24.2nd | 7572/9985 |
| Energy | 44 | 12.1st | 8779/9985 |

The Veneta Dark Star scores in the 99th percentile for improvisation and 98th for build quality, but only 12th percentile for energy and 24th for tightness. It's one of the most temporally developed and best-constructed jams ever played, but it's not loud and it's not tight. The canonical "greatest jam ever" is quiet, loose, and builds slowly. The algorithm understands this.

---

*Generated by [setbreak](https://github.com/lexicone42/setbreak) — a Rust CLI for algorithmic analysis of jam-band music libraries. 10 jam scores computed from 189 DSP audio features per track.*

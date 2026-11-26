# Research 12 — An independent review of the sound voices

Date: **2026-11-26**. Issue #20. The engine's way search, contacts, reverberation, mastering and oracle went through four rounds of independent review. The voices that make its sounds had not. This is that review of `crates/osage-sound-voices`, and what came of each finding.

## Method

A reviewer who had not written the voices read each module against its documentation and its cited sources. Each claim was checked with a scratch program that drives a voice and measures what it makes:

- determinism for a seed;
- no NaN, infinity or steady offset (DC);
- staying within −1…1 for any parameter or drive, including extreme and not-a-number ones;
- blocks of 1, 7, 128 and 4 096 samples;
- the cost per sample;
- provenance: nothing copied from a source that does not allow it.

The maintainer's listening notes (the issue's two comments) were located in the code and measured. Every finding below gives the drive or parameter set that shows it. Each was fixed, with a test that fails before the fix, or answered with a reason. The voices' tests pass, the ignored measurements included: `cargo test -p osage-sound-voices -- --include-ignored`.

## Findings and what came of them

| # | Finding | Shown by | What came of it |
|---|---|---|---|
| 1 | **The metro had no running tests.** Its two tests were ignored measurements, yet three constants said "the test of the nominal level holds it". The bogie was 1.7 to 2.3 dB too loud on all six seeds, and the motor 0.8 to 1.0 dB too quiet. | `Bogie` at 40 km/h, seeds 1–6 | **Fixed.** The tests the constants name now run: `each_voice_is_at_the_nominal_level_where_its_documentation_says` (bogie, motor and span within 0.5 dB, joint peak 0.5 within 0.05), `at_rest_each_falls_silent_and_the_length_of_the_blocks_changes_nothing`. The bogie and motor levels are retuned. |
| 2 | **The metro motor had a steady offset** of 3 to 8 % of its RMS. The Chebyshev polynomials that make its harmonics assume a sine, and are fed a pair of parabolas. The gear term added one too. | `Motor` at 5–70 km/h, effort 0.5 | **Fixed.** The polynomial's mean over the parabolic wave (8/15 and 128/315) is taken off. A 5 Hz follower takes off the rest, which the beat at three times the current makes. `the_motor_pushes_no_way_for_long`: mean under 0.5 % of the RMS. |
| 3 | **The metro traction sounds like a motorbike changing gear.** Between changes of pulse mode the pitch climbs ×1.7, then drops to 0.56–0.6 within 45 ms, five times between 10 and 63 km/h. The tone is a rich harmonic series, with a beat and a gear whine. Its documentation said six harmonics where the code has four. | `Motor` ramped 0–70 km/h | **Answered; documentation fixed.** The steps are what a synchronous GTO drive does. How rich and how abrupt they are on an MP 89 is not known: no measurement was found. The maintainer asked for it to be redone against a recording of line 6, and recordings can now be played (#22). It is not retuned blind. |
| 4 | **The car at 30 km/h drones like a propeller**: 66 % of its power is one line, its firing note at 56 Hz. The European road model puts two thirds of a car's unweighted power in the 63 Hz propulsion octave, and the voice gave all of it to the firing's band. | `Car` at 30 km/h | **Fixed.** The road's noise, kept under the firing note by two low-passes, rumbles in that band beside the firing: the exhaust and the intake. The band levels are measured again (`calibrate`). `the_firing_note_is_not_a_drone`: one line holds under a half of the power (about a third, over seeds), and every octave is still at the model's level within 2 dB. |
| 5 | **The river is exaggerated.** It laps every 1 to 4 s whatever its load, still water included. 60 % of its bubbles come in a gust after each lap. Three quarters of the bank's power rises in pitch and chirps; the paper suggests 0.9 for sparse sounds, where the voice took 0.5. The limiter it says is idle worked 53 times a minute at the ordinary load. | `Water` at load 0, 0.5 and 1 | **Fixed:** <ul><li>Laps follow the water: none on still water, twice as many in a wake. The default is once every 4 to 16 s.</li><li>A fifth of the bubbles, not 60 %, follow a lap.</li><li>Only the shallowest bubbles rise, and laps' bubbles rise a little.</li><li>The wash is halved, and the gulp is lower.</li><li>The largest bubble swings to 0.5, not 0.85.</li></ul> Tests: `a_lap_comes_every_four_to_sixteen_seconds_and_follows_the_water`, `on_an_ordinary_day_the_limiter_is_seldom_at_work` (once a minute at most). The bridge's water is 4 dB down and laps as seldom as the default; it stays louder than a real river beside traffic, as the brief asks. |
| 6 | **The default contact sounds like a struck sheet**: three ringing modes, a gap from 500 Hz to 1 kHz, and a hiss that outlasts them. Its documented peak, 0.5, is 0.34 to 0.43. | `impact`, default parameters | **In part.** <ul><li>A scuff now falls by a factor of e in 7 to 14 ms by hardness, not 15 to 42: it no longer outlasts the ring.</li><li>The documented peak is now 0.4.</li></ul> **Answered:** the modes of the preset matters need checking against recordings before they are changed, as the maintainer noted (#15). |
| 7 | **A soft sole's blow was cut, not faded**: its scuff outlived the voice's own reckoning of how long the blow lasts. Paving under leather stopped at 72 ms, at −33 dB under its peak. | `Matter::PAVING.touched_by(&Matter::LEATHER)` | **Fixed.** A blow lasts until its scuff and its crackle have died too. `a_soft_sole_s_scuff_dies_out_before_the_blow_is_let_go`: its last sound is 60 dB under its peak. |
| 8 | **One blow of strength NaN silenced an `impact` for good**: every later sample was NaN. | a hit of strength NaN, then good hits | **Fixed.** Not a number is no blow. `a_blow_that_is_not_a_number_is_no_blow`. |
| 9 | **One NaN speed or load made a recorded loop NaN for good.** | `recorded::Loop`, speed NaN for one block | **Fixed.** A state that is not a number is taken for rest. `a_state_that_is_not_a_number_is_rest_for_a_block_and_no_more`. |
| 10 | **A third shot cut the oldest dead**, a step 32 times the normal one. | `recorded::Shot` struck three times | **Fixed.** The oldest fades out over 5 ms. `a_third_shot_fades_the_oldest_out_instead_of_cutting_it`. |
| 11 | **A parameter that was NaN or infinite made NaN** in the car, the scooter, the footsteps, the city bed, the impact, the tone and the noise. | each parameter at NaN, +∞, −∞ | **Fixed** in one place: `Params::get` gives the default for a value that is not a finite number. `a_parameter_that_is_not_a_finite_number_is_its_default` checks every parameter of every kind. |
| 12 | **A blow told for a sample past its block** was struck that much later by the joint, and eight such blows locked the joint silent for 2 s. | `Joint`, hits with `at` = 96 000 | **Fixed.** Every voice strikes such a blow at its block's last sample. `a_joint_told_of_a_blow_past_its_block_strikes_at_its_end_and_never_locks`; the shot's test checks the same. |
| 13 | **A clip at rate 0 panicked**, and one above 48 kHz folded its top down (30 kHz came out at 18 kHz). | `Clip::new` at 0 and 96 kHz | **Fixed.** Rate 0 is silence. Above 48 kHz the clip is low-passed before it is read at 48 kHz. `a_clip_at_no_rate_is_silence_and_one_above_the_engine_s_does_not_fold_down`. |
| 14 | **Cost.** `impact` cost 21 ns a sample with nothing struck; the `tone` reference costs 72 ns, more than a car. | timings, one core | **Fixed** for `impact`: with nothing sounding it returns at once. **Answered** for `tone`: a reference measured against must be exact, and its cost is no device's. |
| — | **Provenance.** No audio file is tracked; nothing looks copied from a game or a proprietary source. The metro credited a GPL project for a table that is the EU directive's own. | reading | **Fixed.** The directive is cited directly. |

## What holds

Measured by the reviewer:

- Every voice gives the same samples twice, and the same in blocks of 1, 7, 128 and 4 096.
- No drive breaks the bounds, however extreme or not a number.
- The road model's coefficients and laws match Directive (EU) 2015/996: 97.0 dB at 30 km/h and 2 dB more at 50.
- The river's bubble frequency, damping and rise match van den Doel (2005). Its amplitude law and depth factor could not be checked from memory.
- The free–free bar's ratios match the textbook.
- The car costs 0.43 times the kart's voice (0.4 documented), and the recorded loops what their card says.

## What follows

The bridge sounds different: its digests are made again, and it passes `verify`. Its loudness is now held to −20.9 LUFS by its peaks rather than −20.0. The changes to the river, the car and the contacts need listening: the maintainer's ears judged them, and should judge them again. The metro's traction and the contacts' modes wait for recordings.

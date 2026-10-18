# Experiments

An experiment is the unit of work in this repository. It asks one question
about regex-engine behavior or performance, answers it with a reproducible
measurement, and records a verdict that can be acted on upstream in `rurex`.

## Lifecycle

1. **Proposal.** State the question, the hypothesis, the baseline (a pinned
   `rurex` revision and, for semantics, CPython `re`), the metric, and the
   pass/fail threshold. Present it to the owner before writing code.
2. **Implementation.** Build the smallest harness that answers the question.
   Keep it self-contained and runnable with one command.
3. **Measurement.** Run the baseline and the prototype on the same inputs and
   the same machine. Record raw numbers, tool versions, and the exact
   commands. Run correctness checks before performance measurements.
4. **Verdict.** State whether the hypothesis held, what the numbers were, and
   what should happen next: propose it for `rurex`, revise and rerun, or
   record a negative result and stop.

A negative result is a result. Record it in the experiment README and in the
session report; open an ADR when the result invalidates a design assumption
that later work would otherwise repeat.

## Layout

    experiments/
      NNNN-slug/
        README.md          question, hypothesis, baseline, procedure, results
        run.sh             or run.py: the single entry point
        inputs/            fixed inputs or a script that generates them
        results/           raw measurements committed with the experiment
        requirements...    pinned dependencies, if any

Number experiments sequentially from `0001`. The slug is short and
hyphenated and names the question, for example `0001-ucs2-prefilter`.

## Rules

- Reproduce from a clean checkout: no dependence on files under `var/`, on
  an activated shell, or on an unrecorded environment variable.
- Pin every tool and dependency that affects the measurement. Record the
  hardware and operating system for timing results.
- Compare like with like. The baseline and prototype use the same inputs,
  the same warm-up policy, and the same number of repetitions.
- Keep raw data. Do not replace measurements with a rounded summary.
- Do not delete an experiment README or its results. Code that is no longer
  needed to reproduce the result may be removed after the verdict is
  recorded.

# LifeDB — agent guide

## Protected main

`main` accepts code only through a merged PR. Squash every merge: one branch, one commit on `main`.

## Workflow

Every change runs the chain `branch -> commit -> PR -> merge+squash -> pull+rebase`:

1. **Branch** off `main`.
2. **Commit** on the branch.
3. **Open a PR** against `main`.
4. **Merge** with squash.
5. **Pull and rebase** onto the updated `main`.

The agent is authorized to run the full chain, merge included.

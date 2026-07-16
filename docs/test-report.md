# git-wt test report

- Version: `git-wt 1.1.1`
- Build: `release`
- Date: `2026-07-16 23:25:10 PST`

## Results

| | Tag | Test | Command | Failure |
|---|---|---|---|---|
| ✅ | HAPPY | no-args lists main | `` git-wt `` |  |
| ✅ | HAPPY | list shows main | `` git-wt list `` |  |
| ✅ | HAPPY | ls alias | `` git-wt ls `` |  |
| ✅ | UNHAPPY | list no-match errors | `` git-wt list zzz `` |  |
| ✅ | HAPPY | add existing local branch | `` git-wt add feature/login `` |  |
| ✅ | HAPPY | list shows new worktree | `` git-wt list `` |  |
| ✅ | HAPPY | list filter keeps index | `` git-wt list logi `` |  |
| ✅ | HAPPY | list --col branch only | `` git-wt list --col 2 logi `` |  |
| ✅ | HAPPY | list --col id+branch | `` git-wt list --col 1,2 logi `` |  |
| ✅ | HAPPY | list --col reorder | `` git-wt list --col 2,1 logi `` |  |
| ✅ | UNHAPPY | list --col bad number | `` git-wt list --col 7 `` |  |
| ✅ | UNHAPPY | list --col non-numeric | `` git-wt list --col x `` |  |
| ✅ | HAPPY | bare --col (no list word) | `` git-wt --col 2 `` |  |
| ✅ | HAPPY | bare -c short flag | `` git-wt -c 1,2 `` |  |
| ✅ | HAPPY | add --name suffix | `` git-wt add feature/logout --name review `` |  |
| ✅ | HAPPY | add --dirname whole leaf | `` git-wt add feature/api --dirname scratch2 `` |  |
| ✅ | HAPPY | add tracks remote-only | `` git-wt add remote-only `` |  |
| ✅ | HAPPY | add --dirname as path | `` git-wt add pathtest --dirname sub/deep `` |  |
| ✅ | HAPPY | add --from a ref (new branch) | `` git-wt add newfrom --from feature/login --dirname ff1 `` |  |
| ✅ | UNHAPPY | add dup dir refused | `` git-wt add feature/login `` |  |
| ✅ | UNHAPPY | add name+dirname conflict | `` git-wt add x -n a --dirname b `` |  |
| ✅ | UNHAPPY | add --name empty | `` git-wt add x -n '' `` |  |
| ✅ | UNHAPPY | add --from needs ref | `` git-wt add x --from `` |  |
| ✅ | HAPPY | add new branch declined | `` git-wt add nope --dirname np1 `` |  |
| ✅ | HAPPY | add --stay accepted | `` git-wt add staybr --dirname stay1 --stay `` |  |
| ✅ | UNHAPPY | picker lists checked-out sep | `` git-wt add `` |  |
| ✅ | UNHAPPY | picker shows a checked-out br | `` git-wt add `` |  |
| ✅ | UNHAPPY | picker errors when all checked out | `` git-wt add  # in a fully-checked-out repo `` |  |
| ✅ | HAPPY | add --from base commit matches ref | `` git rev-parse HEAD  # in ff1 `` |  |
| ✅ | HAPPY | bare N prints path | `` git-wt 1 `` |  |
| ✅ | HAPPY | N path prints path | `` git-wt 1 path `` |  |
| ✅ | HAPPY | N show alias | `` git-wt 1 show `` |  |
| ✅ | UNHAPPY | N switch too many args | `` git-wt 1 switch path `` |  |
| ✅ | UNHAPPY | index 0 errors | `` git-wt 0 `` |  |
| ✅ | UNHAPPY | index over range errors | `` git-wt 99 `` |  |
| ✅ | UNHAPPY | unknown action errors | `` git-wt 1 bogus `` |  |
| ✅ | UNHAPPY | flag on target errors | `` git-wt 1 -n x `` |  |
| ✅ | UNHAPPY | flag on target hints actions | `` git-wt 1 --stat `` |  |
| ✅ | UNHAPPY | legacy show hint | `` git-wt show 1 `` |  |
| ✅ | UNHAPPY | legacy remove hint | `` git-wt remove 1 `` |  |
| ✅ | UNHAPPY | branch-like suggests add | `` git-wt feat/x `` |  |
| ✅ | UNHAPPY | plain unknown no suggest | `` git-wt lsit `` |  |
| ✅ | HAPPY | diff --name-only both sides | `` git-wt 1,3 diff --name-only `` |  |
| ✅ | HAPPY | diff .. keeps main-only file | `` git-wt 1,3 diff .. --name-only `` |  |
| ✅ | HAPPY | diff ... hides main-only file | `` git-wt 1,3 diff ... --name-only `` |  |
| ✅ | HAPPY | diff --stat | `` git-wt 1,3 diff --stat `` |  |
| ✅ | HAPPY | diff --name-status | `` git-wt 1,3 diff --name-status `` |  |
| ✅ | HAPPY | diff -- pathspec limits | `` git-wt 1,3 diff --name-only -- onlylogin.txt `` |  |
| ✅ | UNHAPPY | diff needs two worktrees | `` git-wt 1 diff `` |  |
| ✅ | UNHAPPY | diff old form errors | `` git-wt 1 diff 3 `` |  |
| ✅ | UNHAPPY | diff non-numeric target | `` git-wt 1,x diff `` |  |
| ✅ | UNHAPPY | diff bad index errors | `` git-wt 1,99 diff `` |  |
| ✅ | UNHAPPY | diff against itself errors | `` git-wt 1,1 diff `` |  |
| ✅ | UNHAPPY | diff rejects three targets | `` git-wt 1,3,1 diff `` |  |
| ✅ | UNHAPPY | diff rejects other git flags | `` git-wt 1,3 diff -w `` |  |
| ✅ | UNHAPPY | diff flag error hints git | `` git-wt 1,3 diff -w `` |  |
| ✅ | HAPPY | diff warns on dirty worktree | `` git-wt 1,3 diff --name-only `` |  |
| ✅ | HAPPY | dirty warning points at live | `` git-wt 1,3 diff --name-only `` |  |
| ✅ | HAPPY | ref diff blind on same commit | `` git-wt 1,4 diff --name-only  # empty, as designed `` |  |
| ✅ | HAPPY | live sees uncommitted edit | `` git-wt 1,4 diff live --name-only `` |  |
| ✅ | HAPPY | live sees untracked as add | `` git-wt 1,4 diff live --name-status `` |  |
| ✅ | HAPPY | live counts hunks | `` git-wt 1,4 diff live `` |  |
| ✅ | HAPPY | live summary counts lines | `` git-wt 1,4 diff live `` |  |
| ✅ | HAPPY | live hunks show line numbers | `` git-wt 1,4 diff live hunks `` |  |
| ✅ | HAPPY | live --stat still works | `` git-wt 1,4 diff live --stat `` |  |
| ✅ | HAPPY | live suppresses dirty warn | `` git-wt 1,4 diff live --name-only `` |  |
| ✅ | HAPPY | live honors .gitignore | `` git-wt 1,4 diff live --name-only `` |  |
| ✅ | HAPPY | live -- pathspec limits | `` git-wt 1,4 diff live --name-only -- shared.txt `` |  |
| ✅ | HAPPY | live reports on-disk delete | `` git-wt 1,4 diff live --name-status `` |  |
| ✅ | HAPPY | live delete is not an add | `` git-wt 1,4 diff live hunks `` |  |
| ✅ | HAPPY | live identical is empty out | `` git-wt 1,4 diff live -- .gitignore `` |  |
| ✅ | HAPPY | live identical says so once | `` git-wt 1,4 diff live --name-only -- .gitignore `` |  |
| ✅ | UNHAPPY | live bad flag hints no-index | `` git-wt 1,4 diff live -w `` |  |
| ✅ | UNHAPPY | live hint survives word order | `` git-wt 1,4 diff -w live `` |  |
| ✅ | UNHAPPY | live bad flag drops ref hint | `` git-wt 1,4 diff live -w `` |  |
| ✅ | HAPPY | pathspec 'live' not the mode | `` git-wt 1,4 diff --name-only -- live `` |  |
| ✅ | UNHAPPY | live rejects .. range | `` git-wt 1,4 diff live .. `` |  |
| ✅ | UNHAPPY | live rejects ... range | `` git-wt 1,4 diff live ... `` |  |
| ✅ | UNHAPPY | hunks rejects --stat | `` git-wt 1,4 diff hunks --stat `` |  |
| ✅ | HAPPY | --live dashed form works | `` git-wt 1,4 diff --live --name-only `` |  |
| ✅ | HAPPY | hunks works without live | `` git-wt 1,3 diff hunks `` |  |
| ✅ | HAPPY | meld 2 trees passes both dirs | `` git-wt 1,3 meld `` |  |
| ✅ | HAPPY | meld 3 trees, listed order | `` git-wt 3,1,6 meld `` |  |
| ✅ | UNHAPPY | meld one tree errors | `` git-wt 1 meld `` |  |
| ✅ | UNHAPPY | meld over 3 errors | `` git-wt 1,2,3,4 meld `` |  |
| ✅ | UNHAPPY | meld dup tree errors | `` git-wt 1,1 meld `` |  |
| ✅ | UNHAPPY | meld bad index errors | `` git-wt 1,99 meld `` |  |
| ✅ | UNHAPPY | meld non-numeric list errors | `` git-wt 1,x meld `` |  |
| ✅ | UNHAPPY | meld takes no options | `` git-wt 1,2 meld -x `` |  |
| ✅ | UNHAPPY | list needs an action | `` git-wt 1,2 `` |  |
| ✅ | UNHAPPY | list rejects single-tree verb | `` git-wt 1,2 remove `` |  |
| ✅ | UNHAPPY | meld missing gives install hint | `` git-wt 1,2 meld  # PATH without meld `` |  |
| ✅ | UNHAPPY | remove main refused | `` git-wt 1 remove -y `` |  |
| ✅ | HAPPY | remove other prints nothing | `` git-wt 2 remove -y `` |  |
| ✅ | HAPPY | remove-from-inside prints main | `` git-wt 2 remove -y  # cwd inside it `` |  |
| ✅ | UNHAPPY | remove dirty refused (no -f) | `` git-wt 2 remove -y `` |  |
| ✅ | HAPPY | remove dirty with -f | `` git-wt 2 remove -y -f `` |  |
| ✅ | UNHAPPY | merge needs a source | `` git-wt 1 merge `` |  |
| ✅ | UNHAPPY | merge unknown command | `` git-wt merge 2 `` |  |
| ✅ | UNHAPPY | merge old form errors | `` git-wt 1 merge 2 `` |  |
| ✅ | UNHAPPY | merge unknown source | `` git-wt 1 merge zzz `` |  |
| ✅ | UNHAPPY | merge self refused | `` git-wt 1,1 merge `` |  |
| ✅ | UNHAPPY | merge too many args | `` git-wt 1,6 merge 2 `` |  |
| ✅ | UNHAPPY | merge unknown option | `` git-wt 1,6 merge --rebase `` |  |
| ✅ | UNHAPPY | merge ours+theirs conflict | `` git-wt 1,6 merge ours theirs `` |  |
| ✅ | UNHAPPY | merge dry-run + --no-ff | `` git-wt 1,6 merge dry-run --no-ff `` |  |
| ✅ | UNHAPPY | merge continue takes no arg | `` git-wt 1 merge --continue 2 `` |  |
| ✅ | UNHAPPY | merge continue with a side | `` git-wt 1 merge theirs continue `` |  |
| ✅ | UNHAPPY | merge continue+abort | `` git-wt 1 merge continue abort `` |  |
| ✅ | UNHAPPY | rejection names the flag | `` git-wt 1 merge abort -m x --squash `` |  |
| ✅ | UNHAPPY | merge continue w/o merge | `` git-wt 1 merge --continue `` |  |
| ✅ | UNHAPPY | merge abort w/o merge | `` git-wt 1 merge abort `` |  |
| ✅ | HAPPY | merge with untracked only ok | `` git-wt 5,6 merge `` |  |
| ✅ | UNHAPPY | merge into dirty+untracked refused | `` git-wt 5,6 merge `` |  |
| ✅ | UNHAPPY | merge into dirty refused | `` git-wt 5,6 merge `` |  |
| ✅ | HAPPY | merge into dirty with -f | `` git-wt 5,6 merge -f `` |  |
| ✅ | HAPPY | merge by number | `` git-wt 1,6 merge `` |  |
| ✅ | HAPPY | merge by number moved the files | `` test -f a.txt  # in worktree 1 `` |  |
| ✅ | HAPPY | merge prints no stdout | `` git-wt 2,6 merge `` |  |
| ✅ | HAPPY | merge by branch name | `` git-wt 3 merge feat-a `` |  |
| ✅ | UNHAPPY | merge --ff-only refuses | `` git-wt 3,2 merge --ff-only `` |  |
| ✅ | UNHAPPY | merge conflict reports files | `` git-wt 2,3 merge `` |  |
| ✅ | UNHAPPY | merge conflict hints continue | `` git-wt 2 merge --continue `` |  |
| ✅ | UNHAPPY | second merge while stuck | `` git-wt 2 merge feat-a `` |  |
| ✅ | UNHAPPY | continue with unresolved | `` git-wt 2 merge --continue `` |  |
| ✅ | HAPPY | continue after resolve | `` git-wt 2 merge continue `` |  |
| ✅ | HAPPY | abort a conflicted merge | `` git-wt 3 merge --abort `` |  |
| ✅ | HAPPY | abort clears MERGE_HEAD | `` git rev-parse MERGE_HEAD  # in w-cb2 `` |  |
| ✅ | HAPPY | dry-run clean merge | `` git-wt 3,6 merge dry-run `` |  |
| ✅ | UNHAPPY | dry-run reports a conflict | `` git-wt 3,4 merge dry-run `` |  |
| ✅ | UNHAPPY | dry-run names the file | `` git-wt 3,4 merge dry-run `` |  |
| ✅ | UNHAPPY | dry-run says it touched none | `` git-wt 3,4 merge dry-run `` |  |
| ✅ | UNHAPPY | dry-run -d short form | `` git-wt 3,4 merge -d `` |  |
| ✅ | HAPPY | theirs -t short form | `` git-wt 3,6 merge -t -d `` |  |
| ✅ | HAPPY | dry-run leaves no merge state | `` git rev-parse MERGE_HEAD  # in w-cb2 `` |  |
| ✅ | HAPPY | merged current in itself | `` git-wt 1 merged `` |  |
| ✅ | UNHAPPY | merged branch not in main | `` git-wt 1 merged cb3 `` |  |
| ✅ | HAPPY | merged branch is in cb2 | `` git-wt 3 merged stuckbr `` |  |
| ✅ | UNHAPPY | merged list form dest-first | `` git-wt 1,4 merged `` |  |
| ✅ | HAPPY | merged list form reversed | `` git-wt 3,7 merged `` |  |
| ✅ | UNHAPPY | merged too many args | `` git-wt 1 merged cb3 extra `` |  |
| ✅ | UNHAPPY | merged unknown source | `` git-wt 1 merged zzz `` |  |
| ✅ | UNHAPPY | merged self single form | `` git-wt 1 merged 1 `` |  |
| ✅ | UNHAPPY | merged number wants a list | `` git-wt 1 merged 4 `` |  |
| ✅ | UNHAPPY | merged list too many | `` git-wt 1,4,3 merged `` |  |
| ✅ | UNHAPPY | merged list form extra arg | `` git-wt 1,4 merged extra `` |  |
| ✅ | UNHAPPY | merged list form dup | `` git-wt 1,1 merged `` |  |
| ✅ | UNHAPPY | merged legacy hint | `` git-wt merged 2 `` |  |
| ✅ | HAPPY | merged detached list form | `` git-wt 1,5 merged `` |  |
| ✅ | UNHAPPY | merged detached wants a list | `` git-wt 1 merged 5 `` |  |
| ✅ | HAPPY | list --col 6 | `` git-wt list --col 1,2,6 `` |  |
| ✅ | HAPPY | merge theirs resolves | `` git-wt 3,4 merge theirs `` |  |
| ✅ | HAPPY | theirs took the source's side | `` cat shared.txt  # in w-cb2 `` |  |
| ✅ | HAPPY | merge ours keeps our side | `` git-wt 2 merge cb4 ours `` |  |
| ✅ | HAPPY | ours kept worktree N's side | `` cat shared.txt  # in w-cb1 `` |  |
| ✅ | HAPPY | stuck+theirs declined | `` git-wt 7,4 merge theirs `` |  |
| ✅ | UNHAPPY | declining keeps the stopped merge | `` git rev-parse MERGE_HEAD  # in w-ff2 `` |  |
| ✅ | HAPPY | stuck+theirs accepted | `` git-wt 7,4 merge theirs `` |  |
| ✅ | HAPPY | redo let theirs win | `` cat shared.txt  # in w-ff2 `` |  |
| ✅ | HAPPY | list form dry-run clean | `` git-wt 1,6 merge dry-run `` |  |
| ✅ | UNHAPPY | list form takes options | `` git-wt 2,4 merge dry-run `` |  |
| ✅ | UNHAPPY | list form rejects 3 | `` git-wt 1,6,2 merge `` |  |
| ✅ | UNHAPPY | list form needs an action | `` git-wt 1,6 `` |  |
| ✅ | UNHAPPY | list form malformed | `` git-wt 1, merge `` |  |
| ✅ | UNHAPPY | list form + continue | `` git-wt 1,6 merge continue `` |  |
| ✅ | UNHAPPY | list form + abort short | `` git-wt 1,6 merge -a `` |  |
| ✅ | UNHAPPY | list form bad number | `` git-wt 1,x merge `` |  |
| ✅ | HAPPY | list form merges M into N | `` git-wt 8,6 merge `` |  |
| ✅ | HAPPY | list form moved the files | `` test -f a.txt  # in w-lm `` |  |
| ✅ | HAPPY | merge --squash stages only | `` git-wt 7,6 merge --squash `` |  |
| ✅ | HAPPY | --squash leaves changes staged | `` git diff --cached  # in w-ff `` |  |
| ✅ | HAPPY | version | `` git-wt version `` |  |
| ✅ | HAPPY | --help | `` git-wt --help `` |  |

## Summary

- Passed: **173**
- Failed: **0**
- Total: **173**

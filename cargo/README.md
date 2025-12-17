# dependency failure scenario
- Upstream's packaga bump doesn't affect local build:
    - because there is a cago lock file which locks the git commit
- The failure only happened after a wrong [patch] configuration forced Cargo to re-resolve the git dependency, at which point the HEAD version no longer satisfied the declared version constraint.
    -  any change that triggers re-resolution turns git + version into a sharp edge, because Cargo validates versions but does not backtrack commits.

> Always use remote git repo instead of local path to avoid accidently changing the local repo!
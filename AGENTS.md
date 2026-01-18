# AGENTS.md

Development tips and instructions

## Dev environment tips

- Use the repos_to_explore folder if you have trouble accessing the web or searching for READMEs online
- Use the "scratchpad" folder to play around with different concepts or test if code works if you need to
- These folders prevent you from polluting the main folder with artifacts, or from accidentally embedding inner repos
- The main tooling in this  repo is pnpm (node) and cargo (rust), nothing else
- You (codex cli) are running in wsl. So:
    1. All node and cargo builds will likely fail.
       However you can try to check the syntax using various commands
       It may or may not work
    2. Avoid installing new packages. You can if you need to, but there is no
       gurantee it will work in this environment
- do NOT do any computer-wide manipulations such as npm install -g


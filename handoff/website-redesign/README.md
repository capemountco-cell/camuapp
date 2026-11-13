# camu.cc redesign + Simple Analytics (handoff)

These files are for the **Camu website** Lovable project (its synced GitHub repo),
not for this app repo. Don't merge this folder into `camu-site`.

## Apply in the website repo

1. Replace `src/styles.css` and `src/routes/index.tsx` with the files in this folder.
2. In `src/routes/__root.tsx`, inside the root route's `head: () => ({ ... })`:
   - Change the Google Fonts stylesheet link to
     `https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600&family=Geist+Mono:wght@400;500&display=swap`
   - Add Simple Analytics next to `meta` and `links`:
     ```ts
     scripts: [
       { src: "https://scripts.simpleanalyticscdn.com/latest.js", async: true, "data-hostname": "camu.cc" },
     ],
     ```
3. Run the tests (`src/test/*`) and build, then commit and push to the branch Lovable syncs.
4. Publish in Lovable. Then verify Simple Analytics with `check_installation` for camu.cc.

The site has no Content Security Policy, so nothing else is needed for the script.
Screenshots of the redesign rendered locally are in this folder.

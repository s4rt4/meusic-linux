import pkg from "../../package.json";

/** Static app metadata shown in the About dialog. Version stays in sync with package.json. */
export const APP = {
  name: "meusic",
  version: pkg.version as string,
  description: pkg.description as string,
  license: (pkg.license as string) ?? "MIT",
  author: "s4rt4",
  repo: "https://github.com/s4rt4/meusic",
} as const;

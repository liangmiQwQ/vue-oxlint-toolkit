import { execSync } from 'node:child_process';
import { readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { NapiCli } from '@napi-rs/cli';

const cli = new NapiCli();
const currentDir = fileURLToPath(new URL('.', import.meta.url));
const npmTag = process.env.NPM_TAG ?? 'latest';

// 1. Create npm/<platform> subdirectories from napi.targets in package.json.
await cli.createNpmDirs({
  cwd: currentDir,
  packageJsonPath: './package.json',
});

// 2. Move each platform's .node file from artifacts/ into the right npm/ dir.
await cli.artifacts({
  cwd: currentDir,
  packageJsonPath: './package.json',
});

// 3. Update package.json version + set optionalDependencies pointing to
//    each platform package at the same version.
await cli.prePublish({
  cwd: currentDir,
  packageJsonPath: './package.json',
  tagStyle: 'npm',
  ghRelease: false,
  skipOptionalPublish: true,
});

// 4. Publish each per-platform npm package.
const npmDir = join(currentDir, 'npm');
const platformDirs = await readdir(npmDir);

for (const dir of platformDirs) {
  try {
    execSync(`npm publish --tag ${npmTag} --access public`, {
      cwd: join(npmDir, dir),
      env: process.env,
      stdio: 'inherit',
    });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    if (msg.includes('You cannot publish over the previously published versions')) {
      console.warn(`${dir} already published, skipping`);
    } else {
      throw e;
    }
  }
}

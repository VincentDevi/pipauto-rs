import { spawnSync } from 'node:child_process';

export default function globalTeardown() {
  const composeArguments = [
    '--project-name',
    'pipauto-browser-smoke',
    '--file',
    'compose.browser.yaml',
    'down',
    '--volumes',
    '--remove-orphans',
  ];
  const candidates = [
    {
      command: 'docker',
      probeArguments: ['compose', 'version'],
      arguments: ['compose', ...composeArguments],
    },
    {
      command: 'docker-compose',
      probeArguments: ['version'],
      arguments: composeArguments,
    },
  ];

  for (const candidate of candidates) {
    const probe = spawnSync(candidate.command, candidate.probeArguments, { stdio: 'ignore' });
    if (probe.status !== 0) continue;
    const result = spawnSync(candidate.command, candidate.arguments, { stdio: 'inherit' });
    if (result.status === 0) return;
  }

  throw new Error('Could not remove the disposable browser-test database.');
}
